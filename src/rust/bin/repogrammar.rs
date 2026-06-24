use repogrammar::adapters::filesystem::discovery::FilesystemFileDiscovery;
use repogrammar::adapters::persistence::sqlite::SqliteIndexStore;
use repogrammar::application::indexing::{
    index_repository_with_discovery_and_store, IndexingOutcome, IndexingRequest,
};
use repogrammar::application::repository::{
    repository_doctor_with_storage, repository_state_location, repository_status_with_storage,
    RepositoryDoctorReport, RepositoryDoctorRequest, RepositoryImplementationStatus,
    RepositoryStatus, RepositoryStatusReport, RepositoryStatusRequest,
};
use repogrammar::error::RepoGrammarError;
use repogrammar::interfaces::cli::{run_with_runtime, CliIndexRequest, CliRuntime};

fn main() {
    let runtime = ProductCliRuntime;
    let output = run_with_runtime(std::env::args().skip(1), &runtime);
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

        index_repository_with_discovery_and_store(
            IndexingRequest {
                repository_root: request.repository_root,
                max_file_bytes: request.max_file_bytes,
            },
            &FilesystemFileDiscovery,
            &store,
        )
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["indexing"], "file_manifest_only");

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }
}
