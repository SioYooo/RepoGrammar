use repogrammar::adapters::filesystem::discovery::FilesystemFileDiscovery;
use repogrammar::adapters::filesystem::source_store::FilesystemSourceStore;
use repogrammar::adapters::frameworks::SyntaxFrameworkRoleDetector;
use repogrammar::adapters::parsing::RepoGrammarSourceParser;
use repogrammar::adapters::persistence::sqlite::SqliteIndexStore;
use repogrammar::adapters::semantic_workers::typescript::TypeScriptSemanticWorkerBoundary;
use repogrammar::application::indexing::{
    index_repository_with_discovery_parser_frameworks_families_and_store,
    index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store,
    IndexingOutcome, IndexingRequest,
};
use repogrammar::application::install::{
    execute_install, execute_uninstall, AgentTarget, InstallExecutionContext,
    InstallExecutionOutcome, InstallRequest, InstallScope, McpSelfTestRunner, NativeAgentAction,
    NativeAgentConfigurator, MCP_SERVER_NAME,
};
use repogrammar::application::query::{
    list_code_units, list_families_with_freshness, list_indexed_files,
    lookup_family_with_freshness, FamilyEvidenceFreshnessRequest, FamilyListReport,
    FamilyLookupMode, FamilyLookupReport, IndexedCodeUnitsReport, IndexedFilesReport,
};
use repogrammar::application::repository::{
    repository_doctor_with_storage, repository_state_location, repository_status_with_storage,
    RepositoryDoctorReport, RepositoryDoctorRequest, RepositoryImplementationStatus,
    RepositoryStatus, RepositoryStatusReport, RepositoryStatusRequest,
};
use repogrammar::error::RepoGrammarError;
use repogrammar::interfaces::cli::{
    parse_serve_options, repository_root, run_with_runtime, state_dir_override, CliIndexRequest,
    CliRuntime,
};
use repogrammar::interfaces::mcp::{
    serve_json_lines, McpReadOnlyRuntime, McpServeContext, McpToolName,
};
use repogrammar::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use std::io::Write;
use std::process::{Command, Stdio};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let runtime = ProductCliRuntime;
    if args.first().is_some_and(|command| command == "serve") {
        let status = run_serve_command(&args[1..], &runtime);
        std::process::exit(status);
    }
    let output = run_with_runtime(args, &runtime);
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    std::process::exit(output.status);
}

struct ProductCliRuntime;

impl ProductCliRuntime {
    fn store_for_status_request(
        &self,
        request: &RepositoryStatusRequest,
    ) -> Result<SqliteIndexStore, RepoGrammarError> {
        let location = repository_state_location(request.clone())?;
        Ok(SqliteIndexStore::new(location.state_dir))
    }
}

impl CliRuntime for ProductCliRuntime {
    fn index_repository(
        &self,
        _command: &str,
        request: CliIndexRequest,
    ) -> Result<IndexingOutcome, RepoGrammarError> {
        let status_request = RepositoryStatusRequest {
            path: request.repository_root.clone(),
            state_dir_override: request.state_dir_override.clone(),
        };
        let store = self.store_for_status_request(&status_request)?;
        let status = repository_status_with_storage(status_request, &store)?;
        match status.status {
            RepositoryStatus::NotInitialized => {
                return Err(RepoGrammarError::InvalidInput(
                    "repository is not initialized; run repogrammar init".to_string(),
                ));
            }
            RepositoryStatus::CorruptedManifest => {
                return Err(RepoGrammarError::InvalidInput(
                    "repository manifest is corrupted; run repogrammar doctor".to_string(),
                ));
            }
            RepositoryStatus::Initialized { .. } => {}
        }
        if !status.missing_subdirs.is_empty() {
            return Err(RepoGrammarError::InvalidInput(
                "repository-local state is missing required subdirectories; run repogrammar doctor"
                    .to_string(),
            ));
        }
        if status.storage == RepositoryImplementationStatus::Unhealthy {
            return Err(RepoGrammarError::InvalidInput(
                "repository-local storage is unhealthy; run repogrammar doctor".to_string(),
            ));
        }

        let indexing_request = IndexingRequest {
            repository_root: request.repository_root,
            state_dir_override: request.state_dir_override,
            max_file_bytes: request.max_file_bytes,
        };
        let framework_roles = SyntaxFrameworkRoleDetector;
        let parser = RepoGrammarSourceParser::default();
        if let Some(executable) = request.semantic_worker_executable {
            let worker = TypeScriptSemanticWorkerBoundary::new(executable)
                .with_args(request.semantic_worker_args);
            index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
                indexing_request,
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &parser,
                &framework_roles,
                &worker,
                &store,
            )
        } else {
            index_repository_with_discovery_parser_frameworks_families_and_store(
                indexing_request,
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &parser,
                &framework_roles,
                &store,
            )
        }
    }

    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        repository_status_with_storage(request, &store)
    }

    fn repository_doctor(
        &self,
        request: RepositoryDoctorRequest,
    ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
        let status_request = RepositoryStatusRequest {
            path: request.path.clone(),
            state_dir_override: request.state_dir_override.clone(),
        };
        let store = self.store_for_status_request(&status_request)?;
        repository_doctor_with_storage(request, &store)
    }

    fn indexed_files(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<IndexedFilesReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        list_indexed_files(&store)
    }

    fn indexed_units(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<IndexedCodeUnitsReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        list_code_units(&store)
    }

    fn families(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<FamilyListReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        list_families_with_freshness(
            FamilyEvidenceFreshnessRequest {
                repository_root: request.path.clone(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &FilesystemSourceStore,
        )
    }

    fn family_lookup(
        &self,
        request: RepositoryStatusRequest,
        target: Option<&str>,
        mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        lookup_family_with_freshness(
            FamilyEvidenceFreshnessRequest {
                repository_root: request.path.clone(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &FilesystemSourceStore,
            target,
            mode,
        )
    }

    fn install_agent_integration(
        &self,
        command: &str,
        request: InstallRequest,
        context: InstallExecutionContext,
    ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
        let configurator = ProductNativeAgentConfigurator;
        let self_tester = ProductMcpSelfTester::new();
        match command {
            "install" => execute_install(&request, &context, &configurator, &self_tester),
            "uninstall" => execute_uninstall(&request, &context, &configurator),
            _ => Err(RepoGrammarError::InvalidInput(
                "unknown installer command".to_string(),
            )),
        }
    }
}

impl McpReadOnlyRuntime for ProductCliRuntime {
    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError> {
        <Self as CliRuntime>::repository_status(self, request)
    }

    fn family_lookup(
        &self,
        request: RepositoryStatusRequest,
        target: Option<&str>,
        mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        <Self as CliRuntime>::family_lookup(self, request, target, mode)
    }
}

fn run_serve_command(rest: &[String], runtime: &impl McpReadOnlyRuntime) -> i32 {
    let options = match parse_serve_options(rest) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let current_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            return 1;
        }
    };
    let env_lookup = |key: &str| std::env::var(key).ok();
    let context = McpServeContext {
        repository_root: repository_root(&current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(&env_lookup),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve_json_lines(runtime, &context, stdin.lock(), stdout.lock()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            2
        }
    }
}

struct ProductNativeAgentConfigurator;

impl NativeAgentConfigurator for ProductNativeAgentConfigurator {
    fn add_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        executable_path: &str,
        current_dir: &str,
    ) -> Result<NativeAgentAction, RepoGrammarError> {
        let (program, args) = native_add_command(target, scope, executable_path)?;
        run_native_agent_command(&program, &args, current_dir)?;
        Ok(NativeAgentAction {
            target,
            program,
            args,
        })
    }

    fn remove_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        current_dir: &str,
    ) -> Result<NativeAgentAction, RepoGrammarError> {
        let (program, args) = native_remove_command(target, scope)?;
        run_native_agent_command(&program, &args, current_dir)?;
        Ok(NativeAgentAction {
            target,
            program,
            args,
        })
    }
}

struct ProductMcpSelfTester {
    timeout: std::time::Duration,
}

impl ProductMcpSelfTester {
    fn new() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(5),
        }
    }

    #[cfg(test)]
    fn with_timeout(timeout: std::time::Duration) -> Self {
        Self { timeout }
    }
}

impl McpSelfTestRunner for ProductMcpSelfTester {
    fn self_test(&self, executable_path: &str, current_dir: &str) -> Result<(), RepoGrammarError> {
        let mut child = Command::new(executable_path)
            .args(["serve", "--project", current_dir])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| {
                RepoGrammarError::InvalidInput("failed to launch MCP self-test".to_string())
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            writeln!(
                stdin,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
            )
            .map_err(|_| {
                RepoGrammarError::InvalidInput("failed to write MCP self-test request".to_string())
            })?;
            writeln!(
                stdin,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown"})
            )
            .map_err(|_| {
                RepoGrammarError::InvalidInput("failed to write MCP self-test shutdown".to_string())
            })?;
        }
        let output = wait_with_timeout(child, self.timeout)?;
        if !output.status.success() {
            return Err(RepoGrammarError::InvalidInput(
                "MCP self-test failed".to_string(),
            ));
        }
        let stdout = String::from_utf8(output.stdout).map_err(|_| {
            RepoGrammarError::InvalidInput("MCP self-test output was not UTF-8".to_string())
        })?;
        let first = stdout.lines().next().ok_or_else(|| {
            RepoGrammarError::InvalidInput("MCP self-test returned no output".to_string())
        })?;
        let value: serde_json::Value = serde_json::from_str(first).map_err(|_| {
            RepoGrammarError::InvalidInput("MCP self-test output was not JSON".to_string())
        })?;
        if value["result"]["tools"][0]["name"] == McpToolName::Context.as_str() {
            Ok(())
        } else {
            Err(RepoGrammarError::InvalidInput(
                "MCP self-test did not expose repogrammar_context".to_string(),
            ))
        }
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::Output, RepoGrammarError> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child.wait_with_output().map_err(|_| {
                    RepoGrammarError::InvalidInput(
                        "failed to read MCP self-test output".to_string(),
                    )
                });
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RepoGrammarError::InvalidInput(
                    "MCP self-test timed out".to_string(),
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(_) => {
                return Err(RepoGrammarError::InvalidInput(
                    "failed to wait for MCP self-test".to_string(),
                ));
            }
        }
    }
}

fn native_add_command(
    target: AgentTarget,
    scope: InstallScope,
    executable_path: &str,
) -> Result<(String, Vec<String>), RepoGrammarError> {
    match target {
        AgentTarget::Codex => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "codex project-local install is unsupported by the native codex mcp CLI"
                        .to_string(),
                ));
            }
            Ok((
                "codex".to_string(),
                vec![
                    "mcp".to_string(),
                    "add".to_string(),
                    MCP_SERVER_NAME.to_string(),
                    "--".to_string(),
                    executable_path.to_string(),
                    "serve".to_string(),
                ],
            ))
        }
        AgentTarget::ClaudeCode => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "claude-code project-local install is deferred".to_string(),
                ));
            }
            let scope = claude_scope(scope);
            Ok((
                "claude".to_string(),
                vec![
                    "mcp".to_string(),
                    "add".to_string(),
                    "--scope".to_string(),
                    scope.to_string(),
                    MCP_SERVER_NAME.to_string(),
                    "--".to_string(),
                    executable_path.to_string(),
                    "serve".to_string(),
                ],
            ))
        }
        AgentTarget::AllSupported => Err(RepoGrammarError::InvalidInput(
            "native command requires a concrete agent target".to_string(),
        )),
    }
}

fn native_remove_command(
    target: AgentTarget,
    scope: InstallScope,
) -> Result<(String, Vec<String>), RepoGrammarError> {
    match target {
        AgentTarget::Codex => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "codex project-local uninstall is unsupported by the native codex mcp CLI"
                        .to_string(),
                ));
            }
            Ok((
                "codex".to_string(),
                vec![
                    "mcp".to_string(),
                    "remove".to_string(),
                    MCP_SERVER_NAME.to_string(),
                ],
            ))
        }
        AgentTarget::ClaudeCode => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "claude-code project-local uninstall is deferred".to_string(),
                ));
            }
            Ok((
                "claude".to_string(),
                vec![
                    "mcp".to_string(),
                    "remove".to_string(),
                    "--scope".to_string(),
                    claude_scope(scope).to_string(),
                    MCP_SERVER_NAME.to_string(),
                ],
            ))
        }
        AgentTarget::AllSupported => Err(RepoGrammarError::InvalidInput(
            "native command requires a concrete agent target".to_string(),
        )),
    }
}

fn claude_scope(scope: InstallScope) -> &'static str {
    match scope {
        InstallScope::Global => "user",
        InstallScope::ProjectLocal => "project",
    }
}

fn run_native_agent_command(
    program: &str,
    args: &[String],
    current_dir: &str,
) -> Result<(), RepoGrammarError> {
    let status = Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| {
            RepoGrammarError::InvalidInput(format!("native {program} CLI is unavailable"))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(RepoGrammarError::InvalidInput(format!(
            "native {program} MCP command failed"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repogrammar::application::query::{
        assess_semantic_fact_readiness, list_semantic_facts, SemanticFactReadinessRequest,
    };
    use repogrammar::core::model::{CodeUnitKind, Language, RepositoryRevision, UnknownReasonCode};
    use repogrammar::core::policy::freshness::ClaimInputReadiness;
    use repogrammar::interfaces::mcp::handle_context_call;
    use repogrammar::ports::file_discovery::{
        DiscoveredLanguage, FileDiscovery, FileDiscoveryRequest,
    };
    use repogrammar::ports::parser::{SourceDocument, SourceParser};
    use repogrammar::ports::source_store::{SourceReadRequest, SourceStore};
    use serde_json::Value;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[derive(Debug)]
    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "repogrammar-bin-{prefix}-{}-{}",
                std::process::id(),
                unique_suffix()
            ));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos()
    }

    fn cli_args(command: &str, project: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            command.to_string(),
            "--project".to_string(),
            project.display().to_string(),
        ];
        args.extend(extra.iter().map(|value| value.to_string()));
        args
    }

    fn release_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("typescript")
            .join("release")
            .join("v0_1")
    }

    fn python_release_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("python")
            .join("release")
            .join("v0_1")
    }

    fn copy_release_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&release_fixture_root().join(name), destination);
    }

    fn copy_python_release_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&python_release_fixture_root().join(name), destination);
    }

    fn copy_dir_contents(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("create fixture destination");
        let mut entries = fs::read_dir(source)
            .unwrap_or_else(|error| panic!("read fixture directory {source:?}: {error}"))
            .collect::<Result<Vec<_>, _>>()
            .expect("collect fixture entries");
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let file_type = entry.file_type().expect("fixture entry file type");
            let target = destination.join(entry.file_name());
            if file_type.is_dir() {
                copy_dir_contents(&entry.path(), &target);
            } else if file_type.is_file() {
                fs::copy(entry.path(), target).expect("copy fixture file");
            }
        }
    }

    fn parse_machine_output(
        command: &str,
        output: &repogrammar::interfaces::cli::CliOutput,
        workspace: &TempWorkspace,
    ) -> Value {
        assert_eq!(output.status, 0, "{command} stderr: {}", output.stderr);
        assert!(
            output.stderr.is_empty(),
            "{command} wrote stderr: {}",
            output.stderr
        );
        assert_no_output_leakage(command, &output.stdout, workspace);
        serde_json::from_str(output.stdout.trim())
            .unwrap_or_else(|error| panic!("parse {command} JSON: {error}"))
    }

    fn assert_no_output_leakage(command: &str, output: &str, workspace: &TempWorkspace) {
        assert!(
            !output.contains(workspace.path().to_string_lossy().as_ref()),
            "{command} leaked absolute workspace path: {output}"
        );
        assert!(
            !output.contains(release_fixture_root().to_string_lossy().as_ref()),
            "{command} leaked absolute fixture path: {output}"
        );
        assert!(
            !output.contains(python_release_fixture_root().to_string_lossy().as_ref()),
            "{command} leaked absolute Python fixture path: {output}"
        );
        for snippet in [
            "app.get(",
            "export function",
            "describe(",
            "expect(",
            "return <",
            "/accounts",
            "/users",
            "/health",
            "/lonely",
            "accounts: []",
            "users: []",
            "ok: true",
            "loading: false",
            "Promise.resolve",
            "props.name",
            "props.status",
            "toHaveLength",
            "toBe(true)",
            "from fastapi",
            "@router.",
            "response_model=",
            "BaseModel",
            "mapped_column",
            "@pytest.fixture",
            "importlib.import_module",
            "sys.path.append",
            "client.get(",
            "return {",
            "Depends(",
            "HTTPException",
            "DeclarativeBase",
            "select(",
            "getattr(",
            "setattr(",
            "cpython_ast",
            "STRUCTURAL",
            "FRAMEWORK_HEURISTIC",
            "python-fixture-provider",
            "release_fixture_semantic_support",
            "origin_engine",
        ] {
            assert!(
                !output.contains(snippet),
                "{command} leaked source-like snippet {snippet}: {output}"
            );
        }
    }

    fn assert_unknown_query_json(command: &str, value: &Value) {
        assert_eq!(value["command"], command);
        assert_eq!(value["status"], "UNKNOWN");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["unknowns"][0]["reason"], "InsufficientSupport");
    }

    fn assert_no_claim_payload(command: &str, value: &Value) {
        if let Some(families) = value.get("families") {
            assert!(
                families.as_array().expect("families array").is_empty(),
                "{command} UNKNOWN leaked families: {value}"
            );
        }
        for field in [
            "family",
            "member",
            "members",
            "variation_slots",
            "evidence",
            "output",
            "check",
        ] {
            assert!(
                value.get(field).is_none(),
                "{command} UNKNOWN leaked claim field {field}: {value}"
            );
        }
    }

    fn assert_stored_python_structural_fact(
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        path: &str,
        kind: &str,
        target: &str,
        anchor_kind: &str,
    ) {
        let expected_anchor = format!("python_anchor_kind={anchor_kind}");
        assert!(
            facts.iter().any(|fact| {
                fact.path == path
                    && fact.kind == kind
                    && fact.target.as_deref() == Some(target)
                    && fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "STRUCTURAL"
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == &expected_anchor)
            }),
            "missing Python structural fact {kind} {target} with anchor {anchor_kind}"
        );
    }

    fn assert_no_derived_python_support_for_targets(
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        targets: &[&str],
    ) {
        for target in targets {
            assert!(
                facts.iter().all(|fact| {
                    !(fact.origin_engine == "repogrammar-python-derived"
                        && fact.origin_method == "bounded_ast_anchor_v1"
                        && fact.target.as_deref() == Some(*target))
                }),
                "auxiliary target {target} must not be derived family support"
            );
        }
    }

    fn assert_no_dynamic_boundary_fact_leakage(
        workspace: &TempWorkspace,
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
    ) {
        let debug = format!("{facts:?}");
        for forbidden in [
            workspace.path().to_string_lossy().as_ref(),
            release_fixture_root().to_string_lossy().as_ref(),
            python_release_fixture_root().to_string_lossy().as_ref(),
            "importlib.import_module",
            "sys.path.append",
            "getattr(module",
            "secret=(str",
            "decorator_factory(\"secret\")",
            "setattr(target",
            "Depends(make_dependency",
            "return getattr",
            "django_db",
            "return object",
        ] {
            assert!(
                !debug.contains(forbidden),
                "leaked forbidden dynamic source text {forbidden}"
            );
        }
        for fact in facts {
            assert_repo_relative_json_path(&Value::String(fact.path.clone()));
        }
    }

    fn assert_stored_python_unknown_fact(
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        path: &str,
        reason_code: &str,
        affected_claim: &str,
    ) {
        let reason_assumption = format!("reason_code={reason_code}");
        let claim_assumption = format!("affected_claim={affected_claim}");
        assert!(
            facts.iter().any(|fact| {
                fact.path == path
                    && fact.kind == "UNKNOWN"
                    && fact.target.as_deref() == Some(reason_code)
                    && fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "UNKNOWN"
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == &reason_assumption)
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == &claim_assumption)
            }),
            "missing Python UNKNOWN {reason_code} for {affected_claim}"
        );
    }

    fn assert_targets_blocked_from_claim_input(
        workspace: &TempWorkspace,
        store: &impl repogrammar::ports::index_store::IndexStore,
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        targets: &[&str],
    ) {
        let fact_ids = facts
            .iter()
            .filter(|fact| targets.contains(&fact.target.as_deref().unwrap_or_default()))
            .map(|fact| fact.fact_id.clone())
            .collect::<BTreeSet<_>>();
        for target in targets {
            assert!(
                facts
                    .iter()
                    .any(|fact| fact.target.as_deref() == Some(*target)),
                "missing persisted target {target}"
            );
        }

        let readiness = assess_semantic_fact_readiness(
            SemanticFactReadinessRequest {
                repository_root: workspace.path().display().to_string(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            store,
            &FilesystemSourceStore,
        )
        .expect("assess auxiliary fact readiness");
        let mut checked = BTreeSet::new();
        for fact in readiness.facts {
            if !fact_ids.contains(&fact.fact_id) {
                continue;
            }
            checked.insert(fact.fact_id);
            let ClaimInputReadiness::Blocked { unknown } = fact.readiness else {
                panic!("auxiliary fact must stay blocked from claim input");
            };
            assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
        }
        assert_eq!(checked, fact_ids);
    }

    #[derive(Debug, Clone, Copy)]
    struct PythonExactAnchorSmokeCase {
        fixture: &'static str,
        family_id: &'static str,
        support_target: &'static str,
        evidence_path: &'static str,
        member_role: &'static str,
    }

    const PYTHON_EXACT_ANCHOR_SMOKE_CASES: &[PythonExactAnchorSmokeCase] = &[
        PythonExactAnchorSmokeCase {
            fixture: "positive-strong-evidence",
            family_id: "family:python:fastapi_route:framework_fastapi_route",
            support_target: "fastapi.APIRouter.get",
            evidence_path: "routes.py",
            member_role: "framework:fastapi.route",
        },
        PythonExactAnchorSmokeCase {
            fixture: "fastapi-alias-strong-evidence",
            family_id: "family:python:fastapi_route:framework_fastapi_route",
            support_target: "fastapi.APIRouter.get",
            evidence_path: "routes.py",
            member_role: "framework:fastapi.route",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pytest-strong-evidence",
            family_id: "family:python:pytest_test:framework_pytest_test",
            support_target: "pytest.test",
            evidence_path: "test_api.py",
            member_role: "framework:pytest.test",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pytest-fixture-alias-strong-evidence",
            family_id: "family:python:pytest_fixture:framework_pytest_fixture",
            support_target: "pytest.fixture",
            evidence_path: "conftest.py",
            member_role: "framework:pytest.fixture",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pydantic-basic",
            family_id: "family:python:pydantic_model:framework_pydantic_model",
            support_target: "pydantic.BaseModel",
            evidence_path: "schemas.py",
            member_role: "framework:pydantic.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pydantic-settings-strong-evidence",
            family_id: "family:python:pydantic_model:framework_pydantic_model",
            support_target: "pydantic.BaseSettings",
            evidence_path: "settings.py",
            member_role: "framework:pydantic.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pydantic-settings-package-strong-evidence",
            family_id: "family:python:pydantic_model:framework_pydantic_model",
            support_target: "pydantic_settings.BaseSettings",
            evidence_path: "settings.py",
            member_role: "framework:pydantic.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.select",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.execute",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-scalar-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.scalar",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-scalars-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.scalars",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-commit-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.commit",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-rollback-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.rollback",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-async-session-commit-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.ext.asyncio.AsyncSession.commit",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-async-session-rollback-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.ext.asyncio.AsyncSession.rollback",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-model-strong-evidence",
            family_id: "family:python:sqlalchemy_model:framework_sqlalchemy_model",
            support_target: "sqlalchemy.orm.Mapped",
            evidence_path: "models.py",
            member_role: "framework:sqlalchemy.model",
        },
    ];

    fn assert_content_hash_json(value: &Value) {
        let hash = value.as_str().expect("content hash string");
        let Some(hex) = hash.strip_prefix("sha256:") else {
            panic!("hash missing sha256 prefix: {hash}");
        };
        assert_eq!(hex.len(), 64, "sha256 hash length");
        assert!(
            hex.as_bytes().iter().all(|byte| byte.is_ascii_hexdigit()),
            "sha256 hash must be hex: {hash}"
        );
    }

    fn assert_repo_relative_json_path(value: &Value) {
        let path = value.as_str().expect("path string");
        assert!(!path.is_empty(), "path must not be empty");
        assert!(!path.starts_with('/'), "path must be relative: {path}");
        assert!(
            !path.split('/').any(|component| component == ".."),
            "path must not traverse: {path}"
        );
        assert!(
            !path.contains('\\'),
            "path must use forward slashes: {path}"
        );
        assert!(!path.contains("://"), "path must not be a URI: {path}");
        assert!(
            !path
                .as_bytes()
                .windows(2)
                .any(|window| window[0].is_ascii_alphabetic() && window[1] == b':'),
            "path must not be Windows-absolute: {path}"
        );
    }

    fn assert_python_exact_anchor_family_detail(
        command: &str,
        value: &Value,
        case: PythonExactAnchorSmokeCase,
    ) -> String {
        assert_eq!(value["command"], command);
        assert_eq!(value["implemented"], true);
        assert_eq!(value["family"]["family_id"], case.family_id);
        assert_eq!(value["family"]["classification"], "DOMINANT_PATTERN");
        assert_eq!(value["family"]["support"], 3);
        assert_eq!(value["output"]["mode"], "compact");
        assert_eq!(value["output"]["estimated_evidence_tokens"], 0);
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert!(value["evidence"].as_array().expect("evidence").is_empty());
        assert_eq!(value["unknowns"][0]["reason"], "FrameworkMagic");
        assert_eq!(value["unknowns"][0]["class"], "non_blocking_unknown");
        let members = value["members"].as_array().expect("members");
        assert_eq!(members.len(), 3);
        assert!(members.iter().all(|member| {
            member["family_id"] == case.family_id && member["role"] == case.member_role
        }));
        members[0]["code_unit_id"]
            .as_str()
            .expect("member code unit id")
            .to_string()
    }

    fn assert_python_exact_anchor_evidence(
        command: &str,
        value: &Value,
        case: PythonExactAnchorSmokeCase,
        mode: &str,
        token_budget: Option<u64>,
    ) {
        assert_eq!(value["command"], command);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["family"]["family_id"], case.family_id);
        assert_eq!(value["family"]["support"], 3);
        assert_eq!(value["output"]["mode"], mode);
        match token_budget {
            Some(token_budget) => assert_eq!(value["output"]["token_budget"], token_budget),
            None => assert!(value["output"]["token_budget"].is_null()),
        }
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert_eq!(
            value["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert_eq!(
            value["output"]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );
        assert_eq!(value["output"]["missing_claims"], serde_json::json!([]));
        let evidence = value["evidence"].as_array().expect("evidence");
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0]["family_id"], case.family_id);
        assert_eq!(evidence[0]["path"], case.evidence_path);
        assert_eq!(
            evidence[0]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );
        assert_repo_relative_json_path(&evidence[0]["path"]);
        assert_content_hash_json(&evidence[0]["content_hash"]);
        assert!(
            evidence[0]["start_byte"].as_u64().expect("start")
                < evidence[0]["end_byte"].as_u64().expect("end")
        );
    }

    fn assert_python_stale_unknown(command: &str, value: &Value, family_id: &str) {
        assert_eq!(value["command"], command);
        assert_eq!(value["status"], "UNKNOWN");
        assert_eq!(value["implemented"], true);
        assert!(
            value.get("family").is_none(),
            "stale output must not include family"
        );
        assert!(
            value.get("members").is_none(),
            "stale output must not include members"
        );
        assert!(
            value.get("evidence").is_none(),
            "stale output must not include evidence"
        );
        assert!(
            value.get("check").is_none(),
            "stale output must not include check"
        );
        assert_eq!(value["unknowns"][0]["class"], "blocking_unknown");
        assert_eq!(value["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            value["unknowns"][0]["affected_claim"],
            format!("{family_id}:evidence_freshness")
        );
        assert_eq!(value["unknowns"][0]["recovery"], "run repogrammar sync");
    }

    fn mcp_context_payload(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
        arguments: Value,
    ) -> Value {
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let payload = handle_context_call(runtime, &context, &arguments).expect("MCP payload");
        let payload_text = payload.to_string();
        assert_no_output_leakage("mcp", &payload_text, workspace);
        payload
    }

    fn language_from_discovered(language: DiscoveredLanguage) -> Language {
        match language {
            DiscoveredLanguage::TypeScript | DiscoveredLanguage::TypeScriptReact => {
                Language::TypeScript
            }
            DiscoveredLanguage::JavaScript | DiscoveredLanguage::JavaScriptReact => {
                Language::JavaScript
            }
            DiscoveredLanguage::Python => Language::Python,
            DiscoveredLanguage::PythonConfig => Language::PythonConfig,
        }
    }

    #[cfg(unix)]
    fn semantic_support_worker_script(workspace: &TempWorkspace) -> PathBuf {
        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files for worker fixture");
        let parser = RepoGrammarSourceParser::default();
        let mut messages = Vec::new();
        for file in report.files {
            let source = FilesystemSourceStore
                .read_source(SourceReadRequest {
                    repository_root: workspace.path().display().to_string(),
                    path: file.path.clone(),
                    expected_content_hash: file.content_hash.clone(),
                    max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                })
                .expect("read source for worker fixture");
            let parsed = parser
                .parse(SourceDocument {
                    path: &source.path,
                    language: language_from_discovered(file.language),
                    content_hash: source.content_hash.clone(),
                    repository_revision: RepositoryRevision::new("UNKNOWN")
                        .expect("valid revision"),
                    text: &source.text,
                })
                .expect("parse source for worker fixture");
            for unit in parsed.units.into_iter() {
                let Some((target, engine, engine_version, method, note)) =
                    semantic_support_for_unit(&unit.kind)
                else {
                    continue;
                };
                messages.push(serde_json::json!({
                    "protocol_version": 1,
                    "message_type": "fact",
                    "request_id": "repogrammar-typescript-semantic-worker",
                    "fact_kind": "RESOLVED_IMPORT",
                    "subject": format!("{}#semantic-support", unit.id.as_str()),
                    "target": target,
                    "origin": {
                        "engine": engine,
                        "engine_version": engine_version,
                        "method": method
                    },
                    "certainty": "SEMANTIC",
                    "evidence": {
                        "code_unit_id": unit.id.as_str(),
                        "path": unit.provenance.path,
                        "content_hash": unit.provenance.content_hash.as_str(),
                        "repository_revision": "UNKNOWN",
                        "start_byte": unit.range.start_byte,
                        "end_byte": unit.range.end_byte,
                        "note": note
                    },
                    "assumptions": []
                }));
            }
        }
        messages.push(serde_json::json!({
            "protocol_version": 1,
            "message_type": "end_of_stream",
            "request_id": "repogrammar-typescript-semantic-worker"
        }));
        let ndjson = messages
            .into_iter()
            .map(|message| message.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let worker_script = workspace.path().join("semantic-support-worker.sh");
        fs::write(
            &worker_script,
            format!("#!/bin/sh\n/bin/cat >/dev/null\n/bin/cat <<'EOF'\n{ndjson}\nEOF\n"),
        )
        .expect("write semantic support worker");
        worker_script
    }

    #[cfg(unix)]
    fn semantic_support_for_unit(
        kind: &CodeUnitKind,
    ) -> Option<(
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    )> {
        match kind {
            CodeUnitKind::ExpressRoute => Some((
                "package:express",
                "typescript",
                "6.0.0",
                "compiler_api",
                "compiler resolved Express import target",
            )),
            CodeUnitKind::FastApiRoute => Some((
                "fastapi.APIRouter.get",
                "python-fixture-provider",
                "0.1.0",
                "release_fixture_semantic_support",
                "provider resolved FastAPI route decorator",
            )),
            _ => None,
        }
    }

    #[cfg(unix)]
    fn executable_script(workspace: &TempWorkspace, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = workspace.path().join(name);
        fs::write(&path, body).expect("write executable script");
        let mut permissions = fs::metadata(&path)
            .expect("read executable script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("set executable script mode");
        path
    }

    #[test]
    fn release_fixtures_default_product_smoke_returns_json_without_claim_inflation() {
        const RELEASE_FIXTURES: &[(&str, &str)] = &[
            ("express-basic", "users.ts"),
            ("react-basic", "UserCard.tsx"),
            ("jest-vitest-basic", "users.test.ts"),
            ("mixed-js-ts", "routes.js"),
            ("unknown-low-support", "lonely-route.ts"),
        ];
        const QUERY_COMMANDS: &[&str] =
            &["families", "family", "member", "find", "explain", "check"];

        for (fixture, target) in RELEASE_FIXTURES {
            let workspace = TempWorkspace::new(&format!("release-{fixture}"));
            copy_release_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["command"], "index");
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["generation_id"], "gen-000001");
            assert_eq!(index_json["indexing"], "syntax_only_code_units");
            assert_eq!(index_json["parser"], "syntax_only");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert_eq!(index_json["mining"], "deferred");
            assert!(
                index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
                "fixture {fixture} should index at least one unit"
            );

            let files =
                run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
            let files_json = parse_machine_output("files", &files, &workspace);
            assert_eq!(files_json["command"], "files");
            assert_eq!(files_json["status"], "ok");
            assert_eq!(files_json["implemented"], true);
            assert_eq!(files_json["active_generation"], "gen-000001");
            assert_eq!(files_json["indexing"], "syntax_only_code_units");
            assert!(
                !files_json["files"]
                    .as_array()
                    .expect("files array")
                    .is_empty(),
                "fixture {fixture} should report indexed files"
            );

            let units =
                run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
            let units_json = parse_machine_output("units", &units, &workspace);
            assert_eq!(units_json["command"], "units");
            assert_eq!(units_json["status"], "ok");
            assert_eq!(units_json["implemented"], true);
            assert_eq!(units_json["active_generation"], "gen-000001");
            assert_eq!(units_json["indexing"], "syntax_only_code_units");
            assert_eq!(units_json["semantic_worker"], "deferred");
            assert_eq!(units_json["mining"], "deferred");
            assert!(
                !units_json["units"]
                    .as_array()
                    .expect("units array")
                    .is_empty(),
                "fixture {fixture} should report indexed units"
            );

            for command in QUERY_COMMANDS {
                let output = if *command == "families" {
                    run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
                } else {
                    run_with_runtime(
                        cli_args(command, workspace.path(), &[target, "--json"]),
                        &runtime,
                    )
                };
                let value = parse_machine_output(command, &output, &workspace);
                assert_unknown_query_json(command, &value);
            }

            let doctor =
                run_with_runtime(cli_args("doctor", workspace.path(), &["--json"]), &runtime);
            let doctor_json = parse_machine_output("doctor", &doctor, &workspace);
            assert_eq!(doctor_json["command"], "doctor");
            assert_eq!(doctor_json["checks"]["storage"], "available");
            assert!(doctor_json["checks"].get("schema_version").is_none());
        }
    }

    #[test]
    fn python_release_fixtures_default_product_smoke_returns_json_without_claim_inflation() {
        const RELEASE_FIXTURES: &[(&str, &str, &[&str])] = &[
            (
                "fastapi-basic",
                "app.py",
                &["fastapi_route", "pydantic_model"],
            ),
            (
                "pytest-basic",
                "test_users.py",
                &["pytest_fixture", "pytest_test"],
            ),
            (
                "mixed-python",
                "api.py",
                &["fastapi_route", "pydantic_model", "pytest_test"],
            ),
            (
                "dynamic-unknown",
                "dynamic.py",
                &["function", "fastapi_route"],
            ),
            ("low-support", "lonely.py", &["fastapi_route"]),
        ];
        const QUERY_COMMANDS: &[&str] =
            &["families", "family", "member", "find", "explain", "check"];

        for (fixture, target, expected_kinds) in RELEASE_FIXTURES {
            let workspace = TempWorkspace::new(&format!("python-release-{fixture}"));
            copy_python_release_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["command"], "index");
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["generation_id"], "gen-000001");
            assert_eq!(index_json["indexing"], "syntax_only_code_units");
            assert_eq!(index_json["parser"], "syntax_only");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert_eq!(index_json["mining"], "deferred");
            assert!(
                index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
                "fixture {fixture} should index at least one unit"
            );

            let files =
                run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
            let files_json = parse_machine_output("files", &files, &workspace);
            assert_eq!(files_json["command"], "files");
            assert_eq!(files_json["status"], "ok");
            assert_eq!(files_json["implemented"], true);
            assert_eq!(files_json["active_generation"], "gen-000001");
            assert_eq!(files_json["indexing"], "syntax_only_code_units");
            assert!(
                !files_json["files"]
                    .as_array()
                    .expect("files array")
                    .is_empty(),
                "fixture {fixture} should report indexed files"
            );

            let units =
                run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
            let units_json = parse_machine_output("units", &units, &workspace);
            assert_eq!(units_json["command"], "units");
            assert_eq!(units_json["status"], "ok");
            assert_eq!(units_json["implemented"], true);
            assert_eq!(units_json["active_generation"], "gen-000001");
            assert_eq!(units_json["indexing"], "syntax_only_code_units");
            assert_eq!(units_json["semantic_worker"], "deferred");
            assert_eq!(units_json["mining"], "deferred");
            let unit_kinds = units_json["units"]
                .as_array()
                .expect("units array")
                .iter()
                .filter(|unit| unit["language"] == "python")
                .filter_map(|unit| unit["kind"].as_str())
                .collect::<Vec<_>>();
            for expected_kind in *expected_kinds {
                assert!(
                    unit_kinds.contains(expected_kind),
                    "fixture {fixture} should include Python unit kind {expected_kind}; got {unit_kinds:?}"
                );
            }

            for command in QUERY_COMMANDS {
                let output = if *command == "families" {
                    run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
                } else {
                    run_with_runtime(
                        cli_args(command, workspace.path(), &[target, "--json"]),
                        &runtime,
                    )
                };
                let value = parse_machine_output(command, &output, &workspace);
                assert_unknown_query_json(command, &value);
            }

            let doctor =
                run_with_runtime(cli_args("doctor", workspace.path(), &["--json"]), &runtime);
            let doctor_json = parse_machine_output("doctor", &doctor, &workspace);
            assert_eq!(doctor_json["command"], "doctor");
            assert_eq!(doctor_json["checks"]["storage"], "available");
            assert!(doctor_json["checks"].get("schema_version").is_none());
        }
    }

    #[test]
    fn python_release_dynamic_boundaries_persist_unknowns_without_claims() {
        let workspace = TempWorkspace::new("python-release-dynamic-boundaries");
        copy_python_release_fixture("dynamic-unknown", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["generation_id"], "gen-000001");
        assert_eq!(index_json["semantic_worker"], "deferred");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");

        for (reason_code, affected_claim) in [
            ("DynamicImport", "python_import_resolution"),
            ("RuntimeDependencyInjection", "python_import_resolution"),
            ("RuntimeDependencyInjection", "fastapi_dependency_target"),
            ("ConflictingFacts", "pytest_fixture_binding"),
            ("PytestFixtureInjection", "pytest_fixture_binding"),
            ("FrameworkMagic", "python_call_target"),
            ("FrameworkMagic", "python_framework_identity"),
            ("MonkeyPatch", "python_call_target"),
        ] {
            let path = match reason_code {
                "ConflictingFacts" | "PytestFixtureInjection" => {
                    "tests/sub/test_fixture_boundaries.py"
                }
                _ => "dynamic.py",
            };
            assert_stored_python_unknown_fact(&facts.facts, path, reason_code, affected_claim);
        }
        for target in [
            "pytest.builtin_fixture.tmp_path",
            "pytest.builtin_fixture.capsys",
        ] {
            assert_stored_python_structural_fact(
                &facts.facts,
                "tests/sub/test_fixture_boundaries.py",
                "SYMBOL",
                target,
                "pytest_builtin_fixture_context",
            );
        }
        let framework_identity_unknowns = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.path == "dynamic.py"
                    && fact.kind == "UNKNOWN"
                    && fact.target.as_deref() == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert!(
            framework_identity_unknowns >= 2,
            "dynamic decorator and dynamic pydantic model factory must both remain UNKNOWN"
        );
        assert!(facts.facts.iter().all(|fact| {
            !(fact.path == "tests/sub/test_fixture_boundaries.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.client")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                }))
        }));
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &[
                "DynamicImport",
                "RuntimeDependencyInjection",
                "ConflictingFacts",
                "PytestFixtureInjection",
                "FrameworkMagic",
                "MonkeyPatch",
            ],
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &[
                "DynamicImport",
                "RuntimeDependencyInjection",
                "ConflictingFacts",
                "PytestFixtureInjection",
                "pytest.builtin_fixture.tmp_path",
                "pytest.builtin_fixture.capsys",
                "pytest.fixture.client",
                "FrameworkMagic",
                "MonkeyPatch",
                "pydantic.create_model",
                "pydantic.BaseModel",
            ],
        );

        assert_no_dynamic_boundary_fact_leakage(&workspace, &facts.facts);

        for command in ["families", "find", "family", "member", "explain", "check"] {
            let output = if command == "families" {
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
            } else {
                run_with_runtime(
                    cli_args(command, workspace.path(), &["dynamic.py", "--json"]),
                    &runtime,
                )
            };
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
            assert_no_claim_payload(command, &value);
        }
    }

    #[test]
    fn python_release_dynamic_pytest_fixture_name_stays_unknown_without_claims() {
        let workspace = TempWorkspace::new("python-release-dynamic-fixture-name");
        copy_python_release_fixture("pytest-dynamic-fixture-name", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_stored_python_unknown_fact(
            &facts.facts,
            "conftest.py",
            "PytestFixtureInjection",
            "pytest_fixture_binding",
        );
        assert_stored_python_unknown_fact(
            &facts.facts,
            "test_fixture_names.py",
            "PytestFixtureInjection",
            "pytest_fixture_binding",
        );
        assert!(facts.facts.iter().all(|fact| {
            !(fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.dynamic_client")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_fixture_edge"
                        || assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                }))
        }));
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &["PytestFixtureInjection"],
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &["pytest.fixture.dynamic_client", "PytestFixtureInjection"],
        );
        assert_no_dynamic_boundary_fact_leakage(&workspace, &facts.facts);

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_unknown_query_json("families", &families_json);
        assert_no_claim_payload("families", &families_json);
    }

    #[cfg(unix)]
    #[test]
    fn product_runtime_strong_worker_support_produces_family_then_stale_unknown() {
        let workspace = TempWorkspace::new("product-runtime-positive-family");
        fs::write(
            workspace.path().join("users.ts"),
            "app.get('/users', function listUsers(req, res) { res.json([]); });\n",
        )
        .expect("write users route");
        fs::write(
            workspace.path().join("accounts.ts"),
            "app.get('/accounts', function listAccounts(req, res) { res.json([]); });\n",
        )
        .expect("write accounts route");
        let worker_script = semantic_support_worker_script(&workspace);
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                    semantic_worker_executable: Some("/bin/sh".to_string()),
                    semantic_worker_args: vec![worker_script.display().to_string()],
                },
            )
            .expect("index with semantic support worker");
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::Complete
        );
        assert_eq!(outcome.semantic_facts, 4);

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        let family_id = families_json["families"][0]["family_id"]
            .as_str()
            .expect("family id")
            .to_string();

        let family = run_with_runtime(
            cli_args("family", workspace.path(), &[&family_id, "--json"]),
            &runtime,
        );
        let family_json = parse_machine_output("family", &family, &workspace);
        assert_eq!(family_json["status"], "ok");
        assert_eq!(family_json["family"]["family_id"], family_id);

        let check = run_with_runtime(
            cli_args("check", workspace.path(), &["users.ts", "--json"]),
            &runtime,
        );
        let check_json = parse_machine_output("check", &check, &workspace);
        assert_eq!(check_json["status"], "CONTEXT_ONLY");
        assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");

        fs::write(
            workspace.path().join("users.ts"),
            "app.get('/users', function listChanged(req, res) { res.json(['changed']); });\n",
        )
        .expect("mutate users route");

        let stale = run_with_runtime(
            cli_args("family", workspace.path(), &[&family_id, "--json"]),
            &runtime,
        );
        let stale_json = parse_machine_output("family", &stale, &workspace);
        assert_eq!(stale_json["status"], "UNKNOWN");
        assert_eq!(stale_json["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            stale_json["unknowns"][0]["recovery"],
            "run repogrammar sync"
        );
    }

    #[test]
    fn python_release_fixture_exact_anchors_produce_family_without_worker() {
        for case in PYTHON_EXACT_ANCHOR_SMOKE_CASES {
            let workspace =
                TempWorkspace::new(&format!("python-release-derived-family-{}", case.fixture));
            copy_python_release_fixture(case.fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["command"], "index");
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert_eq!(index_json["generation_id"], "gen-000001");

            let status_request = RepositoryStatusRequest {
                path: workspace.path().display().to_string(),
                state_dir_override: None,
            };
            let store = runtime
                .store_for_status_request(&status_request)
                .expect("open store");
            let facts = list_semantic_facts(&store).expect("list semantic facts");
            let derived_support_facts = facts
                .facts
                .iter()
                .filter(|fact| {
                    fact.origin_engine == "repogrammar-python-derived"
                        && fact.origin_method == "bounded_ast_anchor_v1"
                })
                .collect::<Vec<_>>();
            let target_support_facts = derived_support_facts
                .iter()
                .copied()
                .filter(|fact| fact.target.as_deref() == Some(case.support_target))
                .collect::<Vec<_>>();
            assert_eq!(target_support_facts.len(), 3);
            assert!(target_support_facts.iter().all(|fact| {
                matches!(fact.kind.as_str(), "RESOLVED_CALL" | "SYMBOL" | "TYPE")
                    && fact.certainty == "DATAFLOW_DERIVED"
                    && fact.path == case.evidence_path
                    && fact.start_byte < fact.end_byte
            }));
            assert!(facts.facts.iter().all(|fact| {
                !(fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "DATAFLOW_DERIVED")
            }));
            if case.fixture == "pytest-fixture-alias-strong-evidence" {
                assert!(
                    facts.facts.iter().any(|fact| {
                        fact.path == "test_fixture_names.py"
                            && fact.kind == "SYMBOL"
                            && fact.target.as_deref() == Some("pytest.fixture.api_client")
                            && fact.assumptions.iter().any(|assumption| {
                                assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                            })
                    }),
                    "facts={:?}",
                    facts.facts
                );
                assert!(facts.facts.iter().all(|fact| {
                    !(fact.path == "test_fixture_names.py"
                        && fact.kind == "SYMBOL"
                        && fact.target.as_deref() == Some("pytest.fixture._api_client")
                        && fact.assumptions.iter().any(|assumption| {
                            assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                        }))
                }));
            }

            let families = run_with_runtime(
                cli_args("families", workspace.path(), &["--json"]),
                &runtime,
            );
            let families_json = parse_machine_output("families", &families, &workspace);
            assert_eq!(families_json["status"], "ok");
            assert_eq!(families_json["command"], "families");
            assert_eq!(families_json["implemented"], true);
            assert_eq!(families_json["active_generation"], "gen-000001");
            assert_eq!(
                families_json["families"]
                    .as_array()
                    .expect("families")
                    .len(),
                1
            );
            let family_id = families_json["families"][0]["family_id"]
                .as_str()
                .expect("family id")
                .to_string();
            assert_eq!(family_id, case.family_id);
            assert_eq!(
                families_json["families"][0]["classification"],
                "DOMINANT_PATTERN"
            );
            assert_eq!(families_json["families"][0]["support"], 3);

            let family = run_with_runtime(
                cli_args("family", workspace.path(), &[&family_id, "--json"]),
                &runtime,
            );
            let family_json = parse_machine_output("family", &family, &workspace);
            assert_eq!(family_json["status"], "ok");
            let member_id = assert_python_exact_anchor_family_detail("family", &family_json, *case);

            let member = run_with_runtime(
                cli_args("member", workspace.path(), &[&member_id, "--json"]),
                &runtime,
            );
            let member_json = parse_machine_output("member", &member, &workspace);
            assert_eq!(member_json["status"], "ok");
            assert_python_exact_anchor_family_detail("member", &member_json, *case);

            for command in ["find", "explain"] {
                let output = run_with_runtime(
                    cli_args(command, workspace.path(), &[case.evidence_path, "--json"]),
                    &runtime,
                );
                let value = parse_machine_output(command, &output, &workspace);
                assert_eq!(value["status"], "ok", "{command} should find family");
                assert_python_exact_anchor_family_detail(command, &value, *case);
            }

            let check = run_with_runtime(
                cli_args("check", workspace.path(), &[case.evidence_path, "--json"]),
                &runtime,
            );
            let check_json = parse_machine_output("check", &check, &workspace);
            assert_eq!(check_json["status"], "CONTEXT_ONLY");
            assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");
            assert_eq!(
                check_json["check"]["reason"],
                "runtime equivalence remains unproven"
            );
            assert_eq!(check_json["check"]["fail_on"], "none");
            assert_python_exact_anchor_family_detail("check", &check_json, *case);

            let family_auto_evidence = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[&family_id, "--token-budget", "1", "--json"],
                ),
                &runtime,
            );
            let auto_evidence_json =
                parse_machine_output("family", &family_auto_evidence, &workspace);
            assert_python_exact_anchor_evidence(
                "family",
                &auto_evidence_json,
                *case,
                "evidence",
                Some(1),
            );
            assert_eq!(auto_evidence_json["output"]["budget_satisfied"], false);

            let family_compact_override = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[
                        &family_id,
                        "--mode",
                        "compact",
                        "--token-budget",
                        "1",
                        "--json",
                    ],
                ),
                &runtime,
            );
            let compact_override_json =
                parse_machine_output("family", &family_compact_override, &workspace);
            assert_eq!(compact_override_json["status"], "ok");
            assert_eq!(compact_override_json["output"]["mode"], "compact");
            assert_eq!(compact_override_json["output"]["token_budget"], 1);
            assert_eq!(
                compact_override_json["output"]["estimated_evidence_tokens"],
                0
            );
            assert!(compact_override_json["evidence"]
                .as_array()
                .expect("evidence")
                .is_empty());

            let family_evidence = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[
                        &family_id,
                        "--mode",
                        "evidence",
                        "--token-budget",
                        "1",
                        "--json",
                    ],
                ),
                &runtime,
            );
            let evidence_json = parse_machine_output("family", &family_evidence, &workspace);
            assert_python_exact_anchor_evidence(
                "family",
                &evidence_json,
                *case,
                "evidence",
                Some(1),
            );

            let family_deep = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[&family_id, "--mode", "deep", "--json"],
                ),
                &runtime,
            );
            let deep_json = parse_machine_output("family", &family_deep, &workspace);
            assert_python_exact_anchor_evidence("family", &deep_json, *case, "deep", None);
        }
    }

    #[test]
    fn python_release_fixture_records_exact_anchor_variation_evidence() {
        let workspace = TempWorkspace::new("python-release-derived-anchor-variation");
        copy_python_release_fixture("fastapi-route-variation", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        let mut derived_targets = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-python-derived"
                    && fact.origin_method == "bounded_ast_anchor_v1"
                    && fact.path == "routes.py"
            })
            .map(|fact| fact.target.as_deref().expect("support target").to_string())
            .collect::<Vec<_>>();
        derived_targets.sort();
        assert_eq!(
            derived_targets,
            vec![
                "fastapi.APIRouter.delete".to_string(),
                "fastapi.APIRouter.get".to_string(),
                "fastapi.FastAPI.post".to_string(),
            ]
        );

        let family_id = "family:python:fastapi_route:framework_fastapi_route";
        let family = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[
                    family_id,
                    "--mode",
                    "evidence",
                    "--include-variations",
                    "--json",
                ],
            ),
            &runtime,
        );
        let family_json = parse_machine_output("family", &family, &workspace);
        assert_eq!(family_json["status"], "ok");
        assert_eq!(family_json["family"]["family_id"], family_id);
        assert_eq!(family_json["family"]["support"], 3);
        assert_eq!(
            family_json["output"]["covered_claims"],
            serde_json::json!(["canonical", "support", "variation"])
        );
        assert_eq!(
            family_json["output"]["missing_claims"],
            serde_json::json!([])
        );
        assert_eq!(family_json["output"]["source_snippets_included"], false);
        assert!(family_json["variation_slots"]
            .as_array()
            .expect("variation slots")
            .iter()
            .any(|slot| slot["slot_id"] == "slot:python_framework_anchor_target"));
        let evidence = family_json["evidence"].as_array().expect("evidence");
        assert_eq!(evidence.len(), 2);
        assert!(evidence.iter().any(|record| {
            record["covered_claims"] == serde_json::json!(["canonical", "support"])
        }));
        assert!(evidence.iter().any(|record| {
            record["covered_claims"] == serde_json::json!(["support", "variation"])
        }));
        for record in evidence {
            assert_eq!(record["family_id"], family_id);
            assert_eq!(record["path"], "routes.py");
            assert_repo_relative_json_path(&record["path"]);
            assert_content_hash_json(&record["content_hash"]);
            assert!(
                record["start_byte"].as_u64().expect("start")
                    < record["end_byte"].as_u64().expect("end")
            );
        }
    }

    #[test]
    fn python_exact_anchor_queries_return_stale_unknown_without_worker() {
        fn assert_stale_queries(
            runtime: &ProductCliRuntime,
            workspace: &TempWorkspace,
            case: PythonExactAnchorSmokeCase,
            member_id: &str,
        ) {
            let families =
                run_with_runtime(cli_args("families", workspace.path(), &["--json"]), runtime);
            let families_json = parse_machine_output("families", &families, workspace);
            assert_python_stale_unknown("families", &families_json, case.family_id);
            assert!(families_json["families"]
                .as_array()
                .expect("families")
                .is_empty());

            for (command, target) in [
                ("family", case.family_id),
                ("member", member_id),
                ("find", case.evidence_path),
                ("explain", case.evidence_path),
                ("check", case.evidence_path),
            ] {
                let output = run_with_runtime(
                    cli_args(command, workspace.path(), &[target, "--json"]),
                    runtime,
                );
                let value = parse_machine_output(command, &output, workspace);
                assert_python_stale_unknown(command, &value, case.family_id);
            }
        }

        let case = *PYTHON_EXACT_ANCHOR_SMOKE_CASES
            .iter()
            .find(|case| case.fixture == "sqlalchemy-model-strong-evidence")
            .expect("SQLAlchemy model exact-anchor case");

        for mode in ["mutated", "deleted"] {
            let workspace =
                TempWorkspace::new(&format!("python-release-derived-family-stale-{mode}"));
            copy_python_release_fixture(case.fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["status"], "complete");

            let family = run_with_runtime(
                cli_args("family", workspace.path(), &[case.family_id, "--json"]),
                &runtime,
            );
            let family_json = parse_machine_output("family", &family, &workspace);
            assert_eq!(family_json["status"], "ok");
            let member_id = assert_python_exact_anchor_family_detail("family", &family_json, case);

            let evidence_path = workspace.path().join(case.evidence_path);
            match mode {
                "mutated" => fs::write(&evidence_path, "# stale replacement\n")
                    .expect("mutate exact-anchor evidence file"),
                "deleted" => {
                    fs::remove_file(&evidence_path).expect("delete exact-anchor evidence file")
                }
                _ => unreachable!("covered stale mode"),
            }

            assert_stale_queries(&runtime, &workspace, case, &member_id);
        }
    }

    #[cfg(unix)]
    #[test]
    fn python_release_fixture_strong_fastapi_support_produces_family_then_stale_unknown() {
        let workspace = TempWorkspace::new("python-release-positive-family");
        copy_python_release_fixture("positive-strong-evidence", workspace.path());
        let worker_script = semantic_support_worker_script(&workspace);
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                    semantic_worker_executable: Some("/bin/sh".to_string()),
                    semantic_worker_args: vec![worker_script.display().to_string()],
                },
            )
            .expect("index Python release fixture with semantic support worker");
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::Complete
        );
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert!(
            outcome.semantic_facts >= 6,
            "Python fixture should store parser/framework facts plus three semantic support facts"
        );
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        let support_facts = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "python-fixture-provider"
                    && fact.origin_method == "release_fixture_semantic_support"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            support_facts.len(),
            3,
            "fixture provider should emit exactly one strong support fact per route"
        );
        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let units_json = parse_machine_output("units", &units, &workspace);
        let route_units = units_json["units"]
            .as_array()
            .expect("units array")
            .iter()
            .filter(|unit| unit["language"] == "python" && unit["kind"] == "fastapi_route")
            .collect::<Vec<_>>();
        assert_eq!(route_units.len(), 3);
        for fact in &support_facts {
            assert_eq!(fact.certainty, "SEMANTIC");
            assert_eq!(fact.origin_engine_version, "0.1.0");
            assert_eq!(fact.target.as_deref(), Some("fastapi.APIRouter.get"));
            assert!(route_units.iter().any(|unit| {
                unit["id"].as_str() == Some(fact.code_unit_id.as_str())
                    && unit["path"].as_str() == Some(fact.path.as_str())
                    && unit["content_hash"].as_str() == Some(fact.content_hash.as_str())
                    && unit["start_byte"].as_u64() == Some(fact.start_byte as u64)
                    && unit["end_byte"].as_u64() == Some(fact.end_byte as u64)
            }));
        }
        assert!(
            facts.facts.iter().all(|fact| {
                !(fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "SEMANTIC")
            }),
            "CPython parser facts must never be promoted to SEMANTIC"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        assert_eq!(
            families_json["families"]
                .as_array()
                .expect("families")
                .len(),
            1
        );
        let family_id = families_json["families"][0]["family_id"]
            .as_str()
            .expect("family id")
            .to_string();
        assert_eq!(
            family_id,
            "family:python:fastapi_route:framework_fastapi_route"
        );
        assert_eq!(families_json["families"][0]["support"], 3);

        for command in ["family", "find", "explain"] {
            let args = if command == "family" {
                vec![family_id.as_str(), "--json"]
            } else {
                vec!["routes.py", "--json"]
            };
            let output = run_with_runtime(cli_args(command, workspace.path(), &args), &runtime);
            let value = parse_machine_output(command, &output, &workspace);
            assert_eq!(value["status"], "ok", "{command} should find family");
            assert_eq!(value["family"]["family_id"], family_id);
            assert_eq!(value["family"]["support"], 3);
            assert_eq!(value["members"].as_array().expect("members").len(), 3);
            assert!(value["members"]
                .as_array()
                .expect("members")
                .iter()
                .all(|member| member["role"] == "framework:fastapi.route"));
            assert_eq!(value["output"]["mode"], "compact");
            assert_eq!(value["output"]["source_snippets_included"], false);
            assert!(value["evidence"].as_array().expect("evidence").is_empty());
            assert_eq!(
                value["unknowns"][0]["reason"], "FrameworkMagic",
                "runtime equivalence must remain non-blocking UNKNOWN"
            );
        }

        let evidence = run_with_runtime(
            cli_args(
                "find",
                workspace.path(),
                &["routes.py", "--mode", "evidence", "--json"],
            ),
            &runtime,
        );
        let evidence_json = parse_machine_output("find", &evidence, &workspace);
        assert_eq!(evidence_json["status"], "ok");
        assert_eq!(evidence_json["output"]["mode"], "evidence");
        assert_eq!(evidence_json["output"]["source_snippets_included"], false);
        assert_eq!(
            evidence_json["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert_eq!(
            evidence_json["output"]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );
        assert_eq!(
            evidence_json["output"]["missing_claims"],
            serde_json::json!([])
        );
        assert_eq!(
            evidence_json["evidence"]
                .as_array()
                .expect("evidence")
                .len(),
            1
        );
        assert_eq!(
            evidence_json["evidence"][0]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );

        let check = run_with_runtime(
            cli_args("check", workspace.path(), &["routes.py", "--json"]),
            &runtime,
        );
        let check_json = parse_machine_output("check", &check, &workspace);
        assert_eq!(check_json["status"], "CONTEXT_ONLY");
        assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");

        fs::write(
            workspace.path().join("routes.py"),
            "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n@router.get(\"/changed\")\ndef changed_route():\n    return []\n",
        )
        .expect("mutate Python route fixture");

        let stale = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[&family_id, "--mode", "evidence", "--json"],
            ),
            &runtime,
        );
        let stale_json = parse_machine_output("family", &stale, &workspace);
        assert_eq!(stale_json["status"], "UNKNOWN");
        assert!(stale_json.get("evidence").is_none());
        assert_eq!(stale_json["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            stale_json["unknowns"][0]["recovery"],
            "run repogrammar sync"
        );
    }

    #[cfg(unix)]
    #[test]
    fn product_mcp_self_test_times_out_and_reaps_hanging_child() {
        let workspace = TempWorkspace::new("product-mcp-self-test-timeout");
        let script = executable_script(&workspace, "hang.sh", "#!/bin/sh\nsleep 10\n");
        let tester = ProductMcpSelfTester::with_timeout(std::time::Duration::from_millis(100));
        let started = std::time::Instant::now();

        let error = tester
            .self_test(
                script.to_str().expect("script path utf8"),
                workspace.path().to_str().expect("workspace path utf8"),
            )
            .expect_err("hanging self-test should time out");

        assert!(matches!(error, RepoGrammarError::InvalidInput(_)));
        assert!(format!("{error}").contains("MCP self-test timed out"));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn product_runtime_indexes_and_reports_storage_status() {
        let workspace = TempWorkspace::new("product-runtime");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["indexed_units"], 1);
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["parser"], "syntax_only");
        assert_eq!(value["semantic_worker"], "deferred");

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        let value: Value = serde_json::from_str(files.stdout.trim()).expect("files JSON");
        assert_eq!(value["command"], "files");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["files"][0]["path"], "a.ts");
        assert!(!files
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        let value: Value = serde_json::from_str(units.stdout.trim()).expect("units JSON");
        assert_eq!(value["command"], "units");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["mining"], "deferred");
        assert_eq!(value["units"][0]["path"], "a.ts");
        assert!(!units
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn product_runtime_indexes_framework_roles_without_query_claims() {
        let workspace = TempWorkspace::new("product-runtime-framework-roles");
        fs::write(
            workspace.path().join("component.tsx"),
            "export function UserCard() { return <section />; }\n",
        )
        .expect("write source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        assert!(!index
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["parser"], "syntax_only");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["semantic_facts"], 1);
        assert_eq!(value["mining"], "deferred");

        for command in ["find", "families", "family", "explain", "check"] {
            let output =
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime);
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            assert!(!output
                .stdout
                .contains(workspace.path().to_string_lossy().as_ref()));
            let unknown: Value = serde_json::from_str(output.stdout.trim()).expect("UNKNOWN JSON");
            assert_eq!(unknown["status"], "UNKNOWN");
            assert_eq!(unknown["command"], command);
            assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");
            assert_eq!(unknown["implemented"], true);
        }
    }

    #[test]
    fn product_runtime_persists_python_parse_facts_without_query_claims() {
        let workspace = TempWorkspace::new("product-runtime-python");
        fs::create_dir_all(workspace.path().join("src/acme/services")).expect("create package");
        fs::write(workspace.path().join("src/acme/__init__.py"), "").expect("write init");
        fs::write(workspace.path().join("src/acme/services/__init__.py"), "")
            .expect("write services init");
        fs::write(
            workspace.path().join("src/acme/services/users.py"),
            "def list_users():\n    return []\n",
        )
        .expect("write users module");
        fs::write(
            workspace.path().join("src/acme/api.py"),
            r#"
from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel
from acme.services import users
from .services import users as relative_users
from acme.missing import value

router = APIRouter()

class UserOut(BaseModel):
    id: int

def get_db():
    return object()

@router.get("/users", response_model=UserOut)
async def list_users(dependency=Depends(get_db)):
    if dependency is None:
        raise HTTPException(status_code=400)
    return []

def test_users(client, missing_fixture):
    assert client.get("/users").status_code == 200
"#,
        )
        .expect("write source");
        fs::write(
            workspace.path().join("src/acme/conftest.py"),
            r#"
import pytest

@pytest.fixture
def client():
    return object()
"#,
        )
        .expect("write conftest");
        fs::write(
            workspace.path().join("pyproject.toml"),
            r#"
[project]
name = "demo-api"

[tool.pytest.ini_options]
testpaths = ["tests", "../secret"]
pythonpath = ["src", "/tmp/secret"]

[tool.pyright]
include = ["src", "tests"]
extraPaths = ["src/lib", "C:/secret"]

[tool.pyrefly]
project_includes = ["src"]
"#,
        )
        .expect("write pyproject");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["semantic_worker"], "deferred");
        assert!(
            value["semantic_facts"].as_u64().unwrap_or_default() > 3,
            "Python parse facts should be stored in addition to framework-role heuristics"
        );

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        let value: Value = serde_json::from_str(files.stdout.trim()).expect("files JSON");
        assert!(value["files"]
            .as_array()
            .expect("files")
            .iter()
            .any(|file| file["path"] == "pyproject.toml" && file["language"] == "python-config"));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        let value: Value = serde_json::from_str(units.stdout.trim()).expect("units JSON");
        assert!(value["units"]
            .as_array()
            .expect("units")
            .iter()
            .any(|unit| {
                unit["path"] == "src/acme/api.py"
                    && unit["language"] == "python"
                    && unit["kind"] == "fastapi_route"
            }));
        assert!(value["units"]
            .as_array()
            .expect("units")
            .iter()
            .any(|unit| {
                unit["path"] == "src/acme/api.py"
                    && unit["language"] == "python"
                    && unit["kind"] == "pydantic_model"
            }));
        assert!(value["units"]
            .as_array()
            .expect("units")
            .iter()
            .any(|unit| {
                unit["path"] == "pyproject.toml"
                    && unit["language"] == "python-config"
                    && unit["kind"] == "project_config"
            }));
        assert!(!units
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "RESOLVED_IMPORT"
                && fact.target.as_deref() == Some("fastapi.APIRouter")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        let repo_local_imports = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.path == "src/acme/api.py"
                    && fact.kind == "RESOLVED_IMPORT"
                    && fact.target.as_deref() == Some("acme.services.users")
                    && fact.certainty == "STRUCTURAL"
                    && fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=repo_local_import_binding"
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(repo_local_imports.len(), 2);
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("UnresolvedImport")
                && fact.certainty == "UNKNOWN"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "reason_code=UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        assert!(facts
            .facts
            .iter()
            .filter(|fact| fact.origin_engine == "python")
            .all(|fact| fact.certainty != "SEMANTIC"));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("src.acme.api")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("scope.imported.APIRouter")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("scope.namespace.UserOut")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "TYPE"
                && fact.target.as_deref() == Some("pydantic.BaseModel")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("fastapi.APIRouter.get")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "TYPE"
                && fact.target.as_deref() == Some("fastapi.response_model.UserOut")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_response_model")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "RESOLVED_CALL"
                && fact.target.as_deref() == Some("client.get")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("fastapi.dependency.get_db")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency_target")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("fastapi.http_exception.status_code.400")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=fastapi_http_exception_status"
                })
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.test")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_test_function")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.client")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                })
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("PytestFixtureInjection")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "UNKNOWN"
        }));
        let has_project_config_summary = facts.facts.iter().any(|fact| {
            fact.path == "pyproject.toml"
                && fact.kind == "PROJECT_CONFIG"
                && fact.target.as_deref() == Some("python.project_config.project_name.demo-api")
                && fact.origin_engine == "python"
                && fact.origin_method == "tomllib"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "not_family_claim_input")
        });
        let has_project_config_missing_dependency_unknown = facts.facts.iter().any(|fact| {
            fact.path == "pyproject.toml"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("MissingDependency")
                && fact.origin_engine == "python"
                && fact.origin_method == "tomllib"
                && fact.certainty == "UNKNOWN"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_project_config")
        });
        assert!(
            has_project_config_summary || has_project_config_missing_dependency_unknown,
            "pyproject.toml must persist either sanitized config facts or a typed provider UNKNOWN"
        );
        if has_project_config_summary {
            assert!(facts.facts.iter().any(|fact| {
                fact.path == "pyproject.toml"
                    && fact.kind == "PROJECT_CONFIG"
                    && fact.target.as_deref() == Some("python.project_config.source_root.src.lib")
                    && fact.certainty == "STRUCTURAL"
            }));
        }
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "FRAMEWORK_ROLE"
                && fact.certainty == "FRAMEWORK_HEURISTIC"
        }));
        let debug = format!("{:?}", facts.facts);
        for forbidden in [
            workspace.path().to_string_lossy().as_ref(),
            "from fastapi",
            "from acme.services",
            "@router.get",
            "response_model=",
            "Depends(get_db",
            "HTTPException(",
            "assert client.get",
            "return object",
            "missing_fixture",
            "../secret",
            "/tmp/secret",
            "C:/secret",
            "project_includes",
        ] {
            assert!(
                !debug.contains(forbidden),
                "leaked forbidden text {forbidden}"
            );
        }

        let readiness = assess_semantic_fact_readiness(
            SemanticFactReadinessRequest {
                repository_root: workspace.path().display().to_string(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &FilesystemSourceStore,
        )
        .expect("assess Python fact readiness");
        assert_eq!(readiness.active_generation, "gen-000001");
        assert_eq!(readiness.facts.len(), facts.facts.len());
        let mut derived_targets = BTreeSet::new();
        let derived_fact_ids = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-python-derived"
                    && fact.origin_method == "bounded_ast_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                let target = fact.target.as_deref().expect("derived target");
                assert!(
                    matches!(
                        target,
                        "fastapi.APIRouter.get"
                            | "pydantic.BaseModel"
                            | "pytest.fixture"
                            | "pytest.test"
                    ),
                    "unexpected derived target {target}"
                );
                derived_targets.insert(target.to_string());
                assert!(fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=false"));
                fact.fact_id.clone()
            })
            .collect::<BTreeSet<_>>();
        assert!(derived_targets.contains("fastapi.APIRouter.get"));
        assert!(derived_targets.contains("pydantic.BaseModel"));
        assert!(derived_targets.contains("pytest.fixture"));
        assert!(derived_targets.contains("pytest.test"));
        for fact in readiness.facts {
            if derived_fact_ids.contains(&fact.fact_id) {
                assert!(matches!(fact.readiness, ClaimInputReadiness::EligibleInput));
            } else {
                let ClaimInputReadiness::Blocked { unknown } = fact.readiness else {
                    panic!("raw Python parser, framework, and config facts must stay blocked");
                };
                assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
            }
        }

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        assert_eq!(families.status, 0);
        let unknown: Value = serde_json::from_str(families.stdout.trim()).expect("UNKNOWN JSON");
        assert_eq!(unknown["status"], "UNKNOWN");
        assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");

        for command in ["find", "family", "member", "explain", "check"] {
            let output = run_with_runtime(
                cli_args(command, workspace.path(), &["src/acme/api.py", "--json"]),
                &runtime,
            );
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            let unknown: Value = serde_json::from_str(output.stdout.trim()).expect("UNKNOWN JSON");
            assert_eq!(unknown["status"], "UNKNOWN");
            assert_eq!(unknown["command"], command);
            assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");
        }

        let sync = run_with_runtime(cli_args("sync", workspace.path(), &["--json"]), &runtime);
        assert_eq!(sync.status, 0);
        assert!(sync.stderr.is_empty());
        let value: Value = serde_json::from_str(sync.stdout.trim()).expect("sync JSON");
        assert_eq!(value["generation_id"], "gen-000002");
        assert!(
            value["semantic_facts"].as_u64().unwrap_or_default() > 3,
            "sync should persist Python parse facts again"
        );

        let facts = list_semantic_facts(&store).expect("list synced semantic facts");
        assert_eq!(facts.active_generation, "gen-000002");
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.client")
                && fact.certainty == "STRUCTURAL"
        }));
    }

    #[test]
    fn product_runtime_persists_fastapi_request_shape_without_support_claims() {
        let workspace = TempWorkspace::new("product-runtime-fastapi-request-shape");
        fs::write(
            workspace.path().join("app.py"),
            r#"
from typing import Annotated

from fastapi import APIRouter, Body, Cookie, Depends, Header, HTTPException, Path, Query
from pydantic import BaseModel

router = APIRouter()

class UserIn(BaseModel):
    email: str

class UserOut(BaseModel):
    id: int

def get_db():
    return object()

@router.post("/users/{user_id}", response_model=UserOut)
def create_user(
    body: UserIn = Body(...),
    user_id: int = Path(...),
    query: str = Query(None),
    request_id: str = Header(...),
    session_id: str = Cookie(None),
    trace_id: Annotated[str, Header()] = "",
    db=Depends(get_db),
):
    if db is None:
        raise HTTPException(status_code=409)
    return UserOut(id=user_id)
"#,
        )
        .expect("write FastAPI source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["generation_id"], "gen-000001");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");

        for (kind, target, anchor_kind) in [
            (
                "TYPE",
                "fastapi.request_body.UserIn",
                "fastapi_request_body_model",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.path.user_id",
                "fastapi_path_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.query.query",
                "fastapi_query_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.header.request_id",
                "fastapi_header_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.header.trace_id",
                "fastapi_header_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.cookie.session_id",
                "fastapi_cookie_param",
            ),
        ] {
            assert_stored_python_structural_fact(&facts.facts, "app.py", kind, target, anchor_kind);
        }

        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "TYPE",
            "fastapi.response_model.UserOut",
            "fastapi_response_model",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "RESOLVED_CALL",
            "fastapi.Depends",
            "fastapi_dependency",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "SYMBOL",
            "fastapi.dependency.get_db",
            "fastapi_dependency_target",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "RESOLVED_CALL",
            "fastapi.HTTPException",
            "fastapi_http_exception",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "SYMBOL",
            "fastapi.http_exception.status_code.409",
            "fastapi_http_exception_status",
        );
        let persisted_auxiliary_targets = [
            "fastapi.request_body.UserIn",
            "fastapi.request_param.path.user_id",
            "fastapi.request_param.query.query",
            "fastapi.request_param.header.request_id",
            "fastapi.request_param.header.trace_id",
            "fastapi.request_param.cookie.session_id",
            "fastapi.response_model.UserOut",
            "fastapi.Depends",
            "fastapi.dependency.get_db",
            "fastapi.HTTPException",
            "fastapi.http_exception.status_code.409",
        ];
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &persisted_auxiliary_targets,
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &[
                "fastapi.request_body.UserIn",
                "fastapi.request_param.path.user_id",
                "fastapi.request_param.query.query",
                "fastapi.request_param.header.request_id",
                "fastapi.request_param.header.trace_id",
                "fastapi.request_param.cookie.session_id",
                "fastapi.response_model.UserOut",
                "fastapi.dependency.get_db",
                "fastapi.Depends",
                "fastapi.HTTPException",
                "fastapi.http_exception.status_code.409",
            ],
        );

        for command in ["families", "find", "family", "explain", "check"] {
            let output = if command == "families" {
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
            } else {
                run_with_runtime(
                    cli_args(command, workspace.path(), &["app.py", "--json"]),
                    &runtime,
                )
            };
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
        }
    }

    #[test]
    fn python_release_sqlalchemy_auxiliary_context_stays_metadata_only() {
        let workspace = TempWorkspace::new("python-release-sqlalchemy-auxiliary-context");
        copy_python_release_fixture("sqlalchemy-basic", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["generation_id"], "gen-000001");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");

        assert_stored_python_structural_fact(
            &facts.facts,
            "models.py",
            "RESOLVED_CALL",
            "sqlalchemy.orm.relationship",
            "sqlalchemy_relationship",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "repository.py",
            "RESOLVED_CALL",
            "sqlalchemy.orm.Session.add",
            "sqlalchemy_session_call",
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &["sqlalchemy.orm.relationship", "sqlalchemy.orm.Session.add"],
        );
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &["sqlalchemy.orm.relationship", "sqlalchemy.orm.Session.add"],
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .iter()
            .any(|family| {
                family["family_id"] == "family:python:sqlalchemy_model:framework_sqlalchemy_model"
                    && family["support"] == 3
            }));

        for command in ["find", "family", "explain", "check"] {
            let output = run_with_runtime(
                cli_args(command, workspace.path(), &["repository.py", "--json"]),
                &runtime,
            );
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
        }
    }

    #[test]
    fn product_runtime_inventory_reads_file_manifest_only_generation() {
        let workspace = TempWorkspace::new("product-runtime-empty-index");
        fs::write(workspace.path().join("README.txt"), "not a TS/JS source\n")
            .expect("write ignored source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["indexed_units"], 0);

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        let value: Value = serde_json::from_str(files.stdout.trim()).expect("files JSON");
        assert_eq!(value["command"], "files");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["files"].as_array().expect("files array").len(), 0);

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        let value: Value = serde_json::from_str(units.stdout.trim()).expect("units JSON");
        assert_eq!(value["command"], "units");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["units"].as_array().expect("units array").len(), 0);
    }

    #[test]
    fn product_runtime_missing_semantic_worker_falls_back_to_syntax_only() {
        let workspace = TempWorkspace::new("product-runtime-worker-missing");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let runtime = ProductCliRuntime;
        let missing_worker = workspace.path().join("missing-worker");
        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: repogrammar::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
                    semantic_worker_executable: Some(missing_worker.display().to_string()),
                    semantic_worker_args: Vec::new(),
                },
            )
            .expect("missing worker should fall back to syntax-only indexing");

        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::FallbackUnavailable
        );
        assert_eq!(
            outcome.warnings,
            vec!["semantic worker fallback: unavailable".to_string()]
        );
        assert!(!outcome.warnings.iter().any(|warning| {
            warning.contains(workspace.path().to_string_lossy().as_ref())
                || warning.contains("missing-worker")
        }));
    }

    #[test]
    fn product_mcp_context_missing_state_returns_fallback_without_creating_state() {
        let workspace = TempWorkspace::new("product-mcp-missing-state");
        let runtime = ProductCliRuntime;
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };

        let response = handle_context_call(
            &runtime,
            &context,
            &serde_json::json!({
                "operation": "find_analogues",
                "target": "src/routes/a.ts",
            }),
        )
        .expect("fallback response");

        assert_eq!(response["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(response["reason"], "repository is not initialized");
        assert!(!workspace.path().join(".repogrammar").exists());
    }

    #[test]
    fn product_mcp_serve_reads_active_query_without_source_leakage() {
        let workspace = TempWorkspace::new("product-mcp-serve");
        fs::write(
            workspace.path().join("component.tsx"),
            "export function UserCard() { return <section />; }\n",
        )
        .expect("write source");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);
        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        let input = format!(
            "{}\n{}\n",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "repogrammar_context",
                    "arguments": {
                        "operation": "check_conformance",
                        "target": "component.tsx"
                    }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "shutdown"
            })
        );
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let mut output = Vec::new();

        serve_json_lines(&runtime, &context, input.as_bytes(), &mut output)
            .expect("serve MCP lines");
        let output = String::from_utf8(output).expect("utf8 MCP output");
        let first_line = output.lines().next().expect("tool response");
        let response: Value = serde_json::from_str(first_line).expect("JSON-RPC response");
        let payload_text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool payload");
        let payload: Value = serde_json::from_str(payload_text).expect("tool payload JSON");

        assert_eq!(payload["status"], "UNKNOWN");
        assert_eq!(payload["unknowns"][0]["reason"], "InsufficientSupport");
        assert!(!payload_text.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!payload_text.contains("export function"));
    }

    #[test]
    fn product_mcp_matches_python_exact_anchor_family_queries_without_source_leakage() {
        let case = *PYTHON_EXACT_ANCHOR_SMOKE_CASES
            .iter()
            .find(|case| case.fixture == "sqlalchemy-model-strong-evidence")
            .expect("SQLAlchemy model exact-anchor case");
        let workspace = TempWorkspace::new("product-mcp-python-exact-anchor");
        copy_python_release_fixture(case.fixture, workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");

        let find = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "find_analogues",
                "target": case.evidence_path,
                "mode": "evidence",
                "token_budget": 1,
            }),
        );
        assert_python_exact_anchor_evidence("find", &find, case, "evidence", Some(1));

        let show = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "show_family",
                "target": case.family_id,
                "mode": "deep",
            }),
        );
        assert_python_exact_anchor_evidence("family", &show, case, "deep", None);

        let explain = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "explain_deviation",
                "target": case.evidence_path,
            }),
        );
        assert_eq!(explain["status"], "ok");
        assert_python_exact_anchor_family_detail("explain", &explain, case);

        let check = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "check_conformance",
                "target": case.evidence_path,
            }),
        );
        assert_eq!(check["status"], "CONTEXT_ONLY");
        assert_eq!(check["check"]["advisory_status"], "UNKNOWN");
        assert_python_exact_anchor_family_detail("check", &check, case);

        let input = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": McpToolName::Context.as_str(),
                    "arguments": {
                        "operation": "find_analogues",
                        "target": case.evidence_path,
                        "mode": "evidence",
                        "token_budget": 1,
                    },
                },
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "shutdown",
            })
        );
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let mut output = Vec::new();
        serve_json_lines(&runtime, &context, input.as_bytes(), &mut output)
            .expect("serve MCP lines");
        let output = String::from_utf8(output).expect("utf8 MCP output");
        assert_no_output_leakage("mcp-jsonrpc", &output, &workspace);
        let lines = output.lines().collect::<Vec<_>>();
        assert!(
            lines.len() >= 2,
            "MCP should return list and tool responses"
        );
        let tools_response: Value = serde_json::from_str(lines[0]).expect("tools/list response");
        assert_eq!(
            tools_response["result"]["tools"][0]["name"],
            McpToolName::Context.as_str()
        );
        assert_eq!(
            tools_response["result"]["tools"]
                .as_array()
                .expect("tools")
                .len(),
            1
        );
        let tool_response: Value = serde_json::from_str(lines[1]).expect("tools/call response");
        assert_eq!(tool_response["result"]["isError"], false);
        assert_eq!(tool_response["result"]["content"][0]["type"], "text");
        let payload_text = tool_response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool payload text");
        assert_no_output_leakage("mcp-jsonrpc-payload", payload_text, &workspace);
        let payload: Value = serde_json::from_str(payload_text).expect("tool payload JSON");
        assert_python_exact_anchor_evidence("find", &payload, case, "evidence", Some(1));

        fs::write(
            workspace.path().join(case.evidence_path),
            "# stale replacement\n",
        )
        .expect("mutate exact-anchor evidence file");
        for (operation, target, command) in [
            ("find_analogues", case.evidence_path, "find"),
            ("show_family", case.family_id, "family"),
            ("explain_deviation", case.evidence_path, "explain"),
            ("check_conformance", case.evidence_path, "check"),
        ] {
            let stale = mcp_context_payload(
                &runtime,
                &workspace,
                serde_json::json!({
                    "operation": operation,
                    "target": target,
                }),
            );
            assert_python_stale_unknown(command, &stale, case.family_id);
        }

        let stale_input = format!(
            "{}\n{}\n",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "stale-family",
                "method": "tools/call",
                "params": {
                    "name": McpToolName::Context.as_str(),
                    "arguments": {
                        "operation": "show_family",
                        "target": case.family_id,
                    },
                },
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "shutdown",
                "method": "shutdown",
            })
        );
        let mut stale_output = Vec::new();
        serve_json_lines(
            &runtime,
            &context,
            stale_input.as_bytes(),
            &mut stale_output,
        )
        .expect("serve stale MCP lines");
        let stale_output = String::from_utf8(stale_output).expect("utf8 stale MCP output");
        assert_no_output_leakage("mcp-jsonrpc-stale", &stale_output, &workspace);
        let stale_lines = stale_output.lines().collect::<Vec<_>>();
        assert_eq!(stale_lines.len(), 2);
        let stale_response: Value =
            serde_json::from_str(stale_lines[0]).expect("stale tools/call response");
        assert_eq!(stale_response["id"], "stale-family");
        assert_eq!(stale_response["result"]["isError"], false);
        let stale_payload_text = stale_response["result"]["content"][0]["text"]
            .as_str()
            .expect("stale tool payload text");
        assert_no_output_leakage("mcp-jsonrpc-stale-payload", stale_payload_text, &workspace);
        let stale_payload: Value =
            serde_json::from_str(stale_payload_text).expect("stale tool payload JSON");
        assert_python_stale_unknown("family", &stale_payload, case.family_id);
        let shutdown_response: Value =
            serde_json::from_str(stale_lines[1]).expect("shutdown response");
        assert_eq!(shutdown_response["id"], "shutdown");
        assert!(shutdown_response["result"].is_null());
    }

    #[test]
    fn native_agent_commands_use_public_mcp_cli_shapes() {
        let (_program, codex_args) =
            native_add_command(AgentTarget::Codex, InstallScope::Global, "/opt/repogrammar")
                .expect("codex add");
        assert_eq!(
            codex_args,
            vec![
                "mcp",
                "add",
                "repogrammar",
                "--",
                "/opt/repogrammar",
                "serve"
            ]
        );
        assert!(native_add_command(
            AgentTarget::Codex,
            InstallScope::ProjectLocal,
            "/opt/repogrammar"
        )
        .is_err());

        let (_program, claude_args) =
            native_add_command(AgentTarget::ClaudeCode, InstallScope::Global, "/opt/rg")
                .expect("claude add");
        assert_eq!(
            claude_args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "repogrammar",
                "--",
                "/opt/rg",
                "serve"
            ]
        );
        assert!(native_add_command(
            AgentTarget::ClaudeCode,
            InstallScope::ProjectLocal,
            "/opt/rg"
        )
        .is_err());

        let (_program, remove_args) =
            native_remove_command(AgentTarget::ClaudeCode, InstallScope::Global)
                .expect("claude remove");
        assert_eq!(
            remove_args,
            vec!["mcp", "remove", "--scope", "user", "repogrammar"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn product_runtime_forwards_semantic_worker_args() {
        let workspace = TempWorkspace::new("product-runtime-worker-args");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let worker_script = workspace.path().join("worker-fallback.sh");
        fs::write(
            &worker_script,
            r#"#!/bin/sh
/bin/cat >/dev/null
/bin/cat <<'EOF'
{"protocol_version":1,"message_type":"worker_error","request_id":"repogrammar-typescript-semantic-worker","error_code":"SEMANTIC_WORKER_UNAVAILABLE","message":"stub unavailable","fallback":{"mode":"syntax_only","certainty":"UNKNOWN"}}
{"protocol_version":1,"message_type":"end_of_stream","request_id":"repogrammar-typescript-semantic-worker"}
EOF
"#,
        )
        .expect("write worker script");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(cli_args("init", workspace.path(), &[]), &runtime);
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: repogrammar::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
                    semantic_worker_executable: Some("/bin/sh".to_string()),
                    semantic_worker_args: vec![worker_script.display().to_string()],
                },
            )
            .expect("worker fallback should keep syntax-only indexing");

        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::FallbackUnavailable
        );
        assert_eq!(
            outcome.warnings,
            vec!["semantic worker fallback: unavailable".to_string()]
        );
        assert!(!outcome
            .warnings
            .iter()
            .any(|warning| warning.contains(worker_script.to_string_lossy().as_ref())));
    }
}
