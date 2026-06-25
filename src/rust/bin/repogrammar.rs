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
            "BaseModel",
            "mapped_column",
            "pytest.fixture",
            "importlib.import_module",
            "client.get(",
            "return {",
            "Depends(",
            "HTTPException",
            "DeclarativeBase",
            "select(",
            "getattr(",
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
            ("dynamic-unknown", "dynamic.py", &["function"]),
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
        struct ExactAnchorCase {
            fixture: &'static str,
            family_id: &'static str,
            support_target: &'static str,
            evidence_path: &'static str,
            member_role: &'static str,
        }

        const CASES: &[ExactAnchorCase] = &[
            ExactAnchorCase {
                fixture: "positive-strong-evidence",
                family_id: "family:python:fastapi_route:framework_fastapi_route",
                support_target: "fastapi.APIRouter.get",
                evidence_path: "routes.py",
                member_role: "framework:fastapi.route",
            },
            ExactAnchorCase {
                fixture: "fastapi-alias-strong-evidence",
                family_id: "family:python:fastapi_route:framework_fastapi_route",
                support_target: "fastapi.APIRouter.get",
                evidence_path: "routes.py",
                member_role: "framework:fastapi.route",
            },
            ExactAnchorCase {
                fixture: "pytest-strong-evidence",
                family_id: "family:python:pytest_test:framework_pytest_test",
                support_target: "pytest.test",
                evidence_path: "test_api.py",
                member_role: "framework:pytest.test",
            },
            ExactAnchorCase {
                fixture: "pydantic-basic",
                family_id: "family:python:pydantic_model:framework_pydantic_model",
                support_target: "pydantic.BaseModel",
                evidence_path: "schemas.py",
                member_role: "framework:pydantic.model",
            },
            ExactAnchorCase {
                fixture: "pydantic-settings-strong-evidence",
                family_id: "family:python:pydantic_model:framework_pydantic_model",
                support_target: "pydantic.BaseSettings",
                evidence_path: "settings.py",
                member_role: "framework:pydantic.model",
            },
            ExactAnchorCase {
                fixture: "pydantic-settings-package-strong-evidence",
                family_id: "family:python:pydantic_model:framework_pydantic_model",
                support_target: "pydantic_settings.BaseSettings",
                evidence_path: "settings.py",
                member_role: "framework:pydantic.model",
            },
            ExactAnchorCase {
                fixture: "sqlalchemy-strong-evidence",
                family_id: "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
                support_target: "sqlalchemy.select",
                evidence_path: "repository.py",
                member_role: "framework:sqlalchemy.repository_method",
            },
        ];

        for case in CASES {
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
            assert_eq!(derived_support_facts.len(), 3);
            assert!(derived_support_facts.iter().all(|fact| {
                matches!(fact.kind.as_str(), "RESOLVED_CALL" | "SYMBOL" | "TYPE")
                    && fact.certainty == "DATAFLOW_DERIVED"
                    && fact.target.as_deref() == Some(case.support_target)
                    && fact.path == case.evidence_path
                    && fact.start_byte < fact.end_byte
            }));
            assert!(facts.facts.iter().all(|fact| {
                !(fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "DATAFLOW_DERIVED")
            }));

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
            assert_eq!(family_id, case.family_id);
            assert_eq!(families_json["families"][0]["support"], 3);

            let family = run_with_runtime(
                cli_args("family", workspace.path(), &[&family_id, "--json"]),
                &runtime,
            );
            let family_json = parse_machine_output("family", &family, &workspace);
            assert_eq!(family_json["status"], "ok");
            assert_eq!(family_json["family"]["family_id"], family_id);
            assert_eq!(family_json["output"]["mode"], "compact");
            assert_eq!(family_json["output"]["source_snippets_included"], false);
            assert_eq!(family_json["members"].as_array().expect("members").len(), 3);
            assert!(family_json["members"]
                .as_array()
                .expect("members")
                .iter()
                .all(|member| member["role"] == case.member_role));
            assert!(family_json["evidence"]
                .as_array()
                .expect("evidence")
                .is_empty());
            assert_eq!(family_json["unknowns"][0]["reason"], "FrameworkMagic");

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
            assert_eq!(evidence_json["status"], "ok");
            assert_eq!(evidence_json["output"]["mode"], "evidence");
            assert_eq!(evidence_json["output"]["token_budget"], 1);
            assert_eq!(evidence_json["output"]["source_snippets_included"], false);
            assert_eq!(
                evidence_json["evidence"]
                    .as_array()
                    .expect("evidence")
                    .len(),
                1
            );
            assert_eq!(evidence_json["evidence"][0]["path"], case.evidence_path);
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
from fastapi import APIRouter
from pydantic import BaseModel
from acme.services import users
from .services import users as relative_users
from acme.missing import value

router = APIRouter()

class UserOut(BaseModel):
    id: int

@router.get("/users")
async def list_users():
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
                && fact.kind == "RESOLVED_CALL"
                && fact.target.as_deref() == Some("client.get")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
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
