use repogrammar::adapters::filesystem::discovery::FilesystemFileDiscovery;
use repogrammar::adapters::filesystem::source_store::FilesystemSourceStore;
use repogrammar::adapters::frameworks::SyntaxFrameworkRoleDetector;
use repogrammar::adapters::parsing::syntax::SyntaxCodeUnitParser;
use repogrammar::adapters::persistence::sqlite::SqliteIndexStore;
use repogrammar::adapters::semantic_workers::typescript::TypeScriptSemanticWorkerBoundary;
use repogrammar::application::indexing::{
    index_repository_with_discovery_parser_frameworks_families_and_store,
    index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store,
    IndexingOutcome, IndexingRequest,
};
use repogrammar::application::query::{
    list_code_units, list_families, list_indexed_files, lookup_family, FamilyListReport,
    FamilyLookupReport, IndexedCodeUnitsReport, IndexedFilesReport,
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
use repogrammar::interfaces::mcp::{serve_json_lines, McpReadOnlyRuntime, McpServeContext};

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
        if let Some(executable) = request.semantic_worker_executable {
            let worker = TypeScriptSemanticWorkerBoundary::new(executable)
                .with_args(request.semantic_worker_args);
            index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
                indexing_request,
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
                &framework_roles,
                &worker,
                &store,
            )
        } else {
            index_repository_with_discovery_parser_frameworks_families_and_store(
                indexing_request,
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
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
        list_families(&store)
    }

    fn family_lookup(
        &self,
        request: RepositoryStatusRequest,
        target: Option<&str>,
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        lookup_family(&store, target)
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
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        <Self as CliRuntime>::family_lookup(self, request, target)
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

#[cfg(test)]
mod tests {
    use super::*;
    use repogrammar::interfaces::mcp::handle_context_call;
    use serde_json::Value;
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
