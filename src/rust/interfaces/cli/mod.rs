//! CLI argument boundary for the `repogrammar` binary.

use crate::application::indexing::IndexingOutcome;
use crate::application::install::{plan_install, AgentTarget, InstallRequest, InstallScope};
use crate::application::repository::{
    init_repository, repository_doctor, repository_logs, repository_status, uninit_repository,
    unlock_repository, RepositoryDoctorCode, RepositoryDoctorFinding, RepositoryDoctorReport,
    RepositoryDoctorRequest, RepositoryDoctorSeverity, RepositoryImplementationStatus,
    RepositoryInitOutcome, RepositoryLifecycleInitRequest, RepositoryLogsReport,
    RepositoryLogsRequest, RepositoryManifestStatus, RepositoryStatus, RepositoryStatusReport,
    RepositoryStatusRequest, RepositoryUninitOutcome, RepositoryUninitRequest,
    RepositoryUnlockReport, RepositoryUnlockRequest,
};
use crate::error::RepoGrammarError;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliIndexRequest {
    pub repository_root: String,
    pub state_dir_override: Option<String>,
    pub max_file_bytes: u64,
}

pub trait CliRuntime {
    fn index_repository(
        &self,
        command: &str,
        request: CliIndexRequest,
    ) -> Result<IndexingOutcome, RepoGrammarError>;

    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError>;

    fn repository_doctor(
        &self,
        request: RepositoryDoctorRequest,
    ) -> Result<RepositoryDoctorReport, RepoGrammarError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeferredCliRuntime;

impl CliRuntime for DeferredCliRuntime {
    fn index_repository(
        &self,
        command: &str,
        _request: CliIndexRequest,
    ) -> Result<IndexingOutcome, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented(match command {
            "sync" => "sync",
            _ => "index",
        }))
    }

    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError> {
        repository_status(request)
    }

    fn repository_doctor(
        &self,
        request: RepositoryDoctorRequest,
    ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
        repository_doctor(request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CliOutput {
    fn success(stdout: impl Into<String>) -> Self {
        Self {
            status: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn failure(status: i32, stderr: impl Into<String>) -> Self {
        Self {
            status,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

pub fn run<I, S>(args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_with_runtime(args, &DeferredCliRuntime)
}

pub fn run_with_runtime<I, S>(args: I, runtime: &impl CliRuntime) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let current_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir,
        Err(error) => {
            return CliOutput::failure(1, format!("failed to read current directory: {error}\n"));
        }
    };
    let env_lookup = |key: &str| std::env::var(key).ok();
    run_with_context_and_runtime(args, &current_dir, &env_lookup, runtime)
}

#[cfg(test)]
fn run_with_context<I, S, F>(args: I, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    run_with_context_and_runtime(args, current_dir, env_lookup, &DeferredCliRuntime)
}

fn run_with_context_and_runtime<I, S, F>(
    args: I,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    match args.as_slice() {
        [] => CliOutput::success(usage()),
        [arg] if arg == "--help" || arg == "-h" => CliOutput::success(usage()),
        [arg] if arg == "--version" || arg == "-V" => {
            CliOutput::success(format!("repogrammar {}\n", env!("CARGO_PKG_VERSION")))
        }
        [command] if command == "version" => {
            CliOutput::success(format!("repogrammar {}\n", env!("CARGO_PKG_VERSION")))
        }
        [command] if command == "help" => CliOutput::success(usage()),
        [command, rest @ ..] if is_project_lifecycle_command(command) => {
            handle_project_lifecycle(command, rest, current_dir, env_lookup, runtime)
        }
        [command, rest @ ..] if is_query_command(command) => {
            handle_query(command, rest, current_dir, env_lookup, runtime)
        }
        [command, rest @ ..] if is_installer_command(command) => handle_installer(command, rest),
        [command, rest @ ..] if command == "stats" => handle_stats(rest),
        [command, rest @ ..] if command == "telemetry" => handle_telemetry(rest),
        [command] if is_forbidden_graph_command(command) => CliOutput::failure(
            2,
            format!(
                "repogrammar {command} is not a v0.1 top-level command; pattern-family commands are primary, and future graph navigation must live under a secondary namespace\n"
            ),
        ),
        [unknown, ..] => CliOutput::failure(2, format!("unknown command or option: {unknown}\n")),
    }
}

fn usage() -> String {
    [
        "Usage: repogrammar <command> [options]",
        "",
        "Project lifecycle: init, uninit, index, sync, status, doctor, unlock, logs",
        "Pattern-family queries: find, families, family, member, explain, check, files, units",
        "Agent integration: serve, install, uninstall",
        "Metrics: stats, telemetry",
        "Maintenance: version, help",
        "",
    ]
    .join("\n")
}

fn is_project_lifecycle_command(command: &str) -> bool {
    matches!(
        command,
        "init" | "uninit" | "index" | "sync" | "status" | "doctor" | "unlock" | "logs"
    )
}

fn is_query_command(command: &str) -> bool {
    matches!(
        command,
        "find" | "families" | "family" | "member" | "explain" | "check" | "files" | "units"
    )
}

fn is_installer_command(command: &str) -> bool {
    matches!(command, "serve" | "install" | "uninstall")
}

fn is_forbidden_graph_command(command: &str) -> bool {
    matches!(
        command,
        "callers" | "callees" | "impact" | "affected" | "node" | "explore"
    )
}

fn handle_project_lifecycle<F>(
    command: &str,
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    if command == "logs" {
        return match parse_logs_options(rest) {
            Ok(options) => handle_logs(&options, current_dir, env_lookup),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }

    let options = match parse_lifecycle_options(command, rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };

    match command {
        "init" => handle_init(&options, current_dir, env_lookup),
        "uninit" => handle_uninit(&options, current_dir, env_lookup),
        "status" => handle_status(&options, current_dir, env_lookup, runtime),
        "doctor" => handle_doctor(&options, current_dir, env_lookup, runtime),
        "unlock" => handle_unlock(&options, current_dir, env_lookup),
        "index" | "sync" => handle_index(command, &options, current_dir, env_lookup, runtime),
        _ => CliOutput::failure(2, format!("unknown project lifecycle command: {command}\n")),
    }
}

fn handle_query<F>(
    command: &str,
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_query_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let request = RepositoryStatusRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };
    let (reason, guidance) = match runtime.repository_status(request) {
        Ok(report) => match report.status {
            RepositoryStatus::Initialized { .. } => (
                "query execution requires pattern-family evidence",
                "run repogrammar index after pattern-family indexing is implemented",
            ),
            RepositoryStatus::NotInitialized => {
                ("repository is not initialized", "run repogrammar init")
            }
            RepositoryStatus::CorruptedManifest => {
                ("repository status is unavailable", "run repogrammar doctor")
            }
        },
        Err(_) => ("repository status is unavailable", "run repogrammar doctor"),
    };

    if options.json {
        return CliOutput::failure(
            2,
            format!(
                "{{\"status\":\"FALLBACK_TO_CODE_SEARCH\",\"reason\":\"{}\",\"guidance\":\"{}\",\"command\":\"{}\",\"implemented\":false}}\n",
                json_string(reason),
                json_string(guidance),
                json_string(command),
            ),
        );
    }

    CliOutput::failure(
        2,
        format!(
            "FALLBACK_TO_CODE_SEARCH\nreason: {reason}\nguidance: {guidance}\ncommand: repogrammar {command} is not implemented yet; query execution requires a validated pattern-family index\n"
        ),
    )
}

fn handle_installer(command: &str, rest: &[String]) -> CliOutput {
    if command == "serve" {
        if let Err(error) = parse_serve_options(rest) {
            return CliOutput::failure(2, format!("{error}\n"));
        }
        return CliOutput::failure(
            2,
            "repogrammar serve is not implemented yet; the v0.1 MCP server must default to read-only behavior\n",
        );
    }

    let request = match parse_install_options(rest) {
        Ok(request) => request,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let plan = plan_install(&request);

    if request.dry_run {
        let mut output = format!(
            "{command} dry-run: target={}, scope={}, telemetry={}\n",
            plan.target.as_str(),
            plan.scope.as_str(),
            if plan.telemetry_enabled { "on" } else { "off" }
        );
        if request.print_config {
            output.push_str("config preview: absolute executable path, MCP self-test, reversible receipt, and marker-fenced instruction edits are required\n");
        }
        CliOutput::success(output)
    } else {
        CliOutput::failure(
            2,
            format!(
                "repogrammar {command} writes are not implemented yet; rerun with --dry-run to inspect the safe integration plan\n"
            ),
        )
    }
}

fn handle_stats(rest: &[String]) -> CliOutput {
    if let Err(error) = reject_unknown_options(rest, &["--json", "--quiet", "--verbose"]) {
        return CliOutput::failure(2, format!("{error}\n"));
    }
    CliOutput::success(
        "stats: no initialized index; token metrics must be classified as MEASURED, DERIVED, ESTIMATED, or CAUSAL_EXPERIMENT, and derived context compression is not actual token savings\n",
    )
}

fn handle_telemetry(rest: &[String]) -> CliOutput {
    match rest {
        [] => CliOutput::success("telemetry: anonymous=off, research-trace=off\n"),
        [command] if command == "status" => {
            CliOutput::success("telemetry: anonymous=off, research-trace=off\n")
        }
        [command] if matches!(command.as_str(), "on" | "off" | "purge" | "export") => {
            CliOutput::failure(
                2,
                format!(
                    "repogrammar telemetry {command} is not implemented yet; telemetry consent and local storage writes are deferred\n"
                ),
            )
        }
        [unknown, ..] => CliOutput::failure(2, format!("unknown telemetry command: {unknown}\n")),
    }
}

fn handle_init<F>(options: &LifecycleOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryLifecycleInitRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        write_root_gitignore: options.write_gitignore,
    };

    match init_repository(request) {
        Ok(outcome) if options.json => CliOutput::success(init_outcome_json(&outcome)),
        Ok(outcome) => CliOutput::success(init_outcome_human(&outcome)),
        Err(error) => lifecycle_error("init", options.json, error),
    }
}

fn handle_uninit<F>(options: &LifecycleOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryUninitRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        yes: options.yes,
    };

    match uninit_repository(request) {
        Ok(outcome) if options.json => CliOutput::success(uninit_outcome_json(&outcome)),
        Ok(outcome) => CliOutput::success(uninit_outcome_human(&outcome)),
        Err(error) => lifecycle_error("uninit", options.json, error),
    }
}

fn handle_status<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryStatusRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };

    match runtime.repository_status(request) {
        Ok(report) if options.json => CliOutput::success(status_json(&report)),
        Ok(report) => CliOutput::success(status_human(&report)),
        Err(error) => lifecycle_error("status", options.json, error),
    }
}

fn handle_doctor<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryDoctorRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };

    match runtime.repository_doctor(request) {
        Ok(report) if options.json => CliOutput::success(doctor_json(&report)),
        Ok(report) => CliOutput::success(doctor_human(&report)),
        Err(error) => lifecycle_error("doctor", options.json, error),
    }
}

fn handle_index<F>(
    command: &str,
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = CliIndexRequest {
        repository_root: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        max_file_bytes: crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
    };

    match runtime.index_repository(command, request) {
        Ok(outcome) if options.json => {
            CliOutput::success(index_outcome_json(command, &outcome, options))
        }
        Ok(outcome) => CliOutput::success(index_outcome_human(command, &outcome, options)),
        Err(RepoGrammarError::NotImplemented(_)) => handle_deferred_long_running(command, options),
        Err(error) => lifecycle_error(command, options.json, error),
    }
}

fn handle_unlock<F>(options: &LifecycleOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    if options.force && !options.yes {
        return CliOutput::failure(
            2,
            "repogrammar unlock --force requires --yes after stale-lock diagnosis\n",
        );
    }

    let request = RepositoryUnlockRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        force: options.force,
        yes: options.yes,
    };

    match unlock_repository(request) {
        Ok(outcome) if options.json => CliOutput::success(unlock_json(&outcome)),
        Ok(outcome) => CliOutput::success(unlock_human(&outcome)),
        Err(error) => lifecycle_error("unlock", options.json, error),
    }
}

fn handle_logs<F>(options: &LogsOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryLogsRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        component: options.component.clone(),
        tail: options.tail,
        since: options.since.clone(),
        redact: options.redact,
    };

    match repository_logs(request) {
        Ok(outcome) if options.json => CliOutput::success(logs_json(&outcome, options)),
        Ok(outcome) => CliOutput::success(logs_human(&outcome)),
        Err(error) => lifecycle_error("logs", options.json, error),
    }
}

fn handle_deferred_long_running(command: &str, options: &LifecycleOptions) -> CliOutput {
    if options.json {
        return CliOutput::failure(
            2,
            format!(
                "{{\"command\":\"{}\",\"status\":\"not_implemented\",\"implemented\":false,\"progress\":\"{}\",\"reason\":\"indexing and sync require discovery, storage, and generation validation\"}}\n",
                json_string(command),
                options.progress.as_str()
            ),
        );
    }

    CliOutput::failure(
        2,
        format!(
            "repogrammar {command} is not implemented yet; progress={}, indexing and sync require discovery, storage, and generation validation\n",
            options.progress.as_str()
        ),
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct QueryOptions {
    project_path: Option<String>,
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgressMode {
    Auto,
    Always,
    Never,
}

impl ProgressMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err("--progress requires auto, always, or never".to_string()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleOptions {
    project_path: Option<String>,
    json: bool,
    quiet: bool,
    verbose: bool,
    progress: ProgressMode,
    write_gitignore: bool,
    yes: bool,
    force: bool,
}

impl Default for LifecycleOptions {
    fn default() -> Self {
        Self {
            project_path: None,
            json: false,
            quiet: false,
            verbose: false,
            progress: ProgressMode::Auto,
            write_gitignore: false,
            yes: false,
            force: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogsOptions {
    project_path: Option<String>,
    json: bool,
    quiet: bool,
    verbose: bool,
    tail: Option<usize>,
    since: Option<String>,
    component: Option<String>,
    redact: bool,
}

impl Default for LogsOptions {
    fn default() -> Self {
        Self {
            project_path: None,
            json: false,
            quiet: false,
            verbose: false,
            tail: None,
            since: None,
            component: None,
            redact: true,
        }
    }
}

fn parse_serve_options(rest: &[String]) -> Result<(), String> {
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--progress" => {
                let value = option_value(rest, index, "--progress", "auto, always, or never")?;
                ProgressMode::parse(value)?;
                index += 2;
            }
            "--json" | "--quiet" | "--verbose" => index += 1,
            value if !value.starts_with('-') => index += 1,
            other => return Err(format!("unknown serve option: {other}")),
        }
    }
    Ok(())
}

fn parse_lifecycle_options(command: &str, rest: &[String]) -> Result<LifecycleOptions, String> {
    let mut options = LifecycleOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--progress" if matches!(command, "init" | "index" | "sync") => {
                let value = option_value(rest, index, "--progress", "auto, always, or never")?;
                options.progress = ProgressMode::parse(value)?;
                index += 2;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--quiet" => {
                options.quiet = true;
                index += 1;
            }
            "--verbose" => {
                options.verbose = true;
                index += 1;
            }
            "--write-gitignore" if command == "init" => {
                options.write_gitignore = true;
                index += 1;
            }
            "--yes" if matches!(command, "uninit" | "unlock") => {
                options.yes = true;
                index += 1;
            }
            "--force" if command == "unlock" => {
                options.force = true;
                index += 1;
            }
            value if !value.starts_with('-') => {
                set_project_path(&mut options.project_path, value)?;
                index += 1;
            }
            other => return Err(format!("unknown {command} option: {other}")),
        }
    }
    Ok(options)
}

fn parse_logs_options(rest: &[String]) -> Result<LogsOptions, String> {
    let mut options = LogsOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--component" => {
                let value = option_value(
                    rest,
                    index,
                    "--component",
                    "index, daemon, mcp, or telemetry",
                )?;
                validate_log_component(value)?;
                options.component = Some(value.to_string());
                index += 2;
            }
            "--since" => {
                let duration = option_value(rest, index, "--since", "a duration")?;
                options.since = Some(duration.to_string());
                index += 2;
            }
            "--tail" => {
                options.tail = Some(100);
                if let Some(value) = rest.get(index + 1).filter(|value| !value.starts_with('-')) {
                    options.tail = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| "--tail requires a non-negative integer".to_string())?,
                    );
                    index += 2;
                } else {
                    index += 1;
                }
            }
            "--redact" => {
                options.redact = true;
                index += 1;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--quiet" => {
                options.quiet = true;
                index += 1;
            }
            "--verbose" => {
                options.verbose = true;
                index += 1;
            }
            value if !value.starts_with('-') => {
                set_project_path(&mut options.project_path, value)?;
                index += 1;
            }
            other => return Err(format!("unknown logs option: {other}")),
        }
    }
    Ok(options)
}

fn parse_query_options(rest: &[String]) -> Result<QueryOptions, String> {
    let mut options = QueryOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" => {
                let value = option_value(rest, index, "--project", "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--token-budget" => {
                option_value(rest, index, "--token-budget", "a token budget")?;
                index += 2;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--include-variations" | "--include-exceptions" => index += 1,
            value if !value.starts_with('-') => index += 1,
            other => return Err(format!("unknown query option: {other}")),
        }
    }
    Ok(options)
}

fn parse_install_options(rest: &[String]) -> Result<InstallRequest, String> {
    let mut request = InstallRequest::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--target" => {
                let Some(value) = rest.get(index + 1) else {
                    return Err("--target requires a value".to_string());
                };
                request.target = AgentTarget::parse(value)?;
                index += 2;
            }
            "--scope" => {
                let Some(value) = rest.get(index + 1) else {
                    return Err("--scope requires global or project".to_string());
                };
                request.scope = InstallScope::parse(value)?;
                index += 2;
            }
            "--dry-run" => {
                request.dry_run = true;
                index += 1;
            }
            "--yes" => {
                request.assume_yes = true;
                index += 1;
            }
            "--print-config" => {
                request.print_config = true;
                index += 1;
            }
            "--no-telemetry" => {
                request.telemetry_enabled = false;
                index += 1;
            }
            "--no-permissions" => {
                request.no_permissions = true;
                index += 1;
            }
            other => return Err(format!("unknown installer option: {other}")),
        }
    }
    Ok(request)
}

fn reject_unknown_options(rest: &[String], allowed: &[&str]) -> Result<(), String> {
    for option in rest {
        if !allowed.contains(&option.as_str()) {
            return Err(format!("unknown option: {option}"));
        }
    }
    Ok(())
}

fn option_value<'a>(
    rest: &'a [String],
    index: usize,
    option: &str,
    expectation: &str,
) -> Result<&'a str, String> {
    let Some(value) = rest.get(index + 1) else {
        return Err(format!("{option} requires {expectation}"));
    };
    if value.starts_with('-') {
        return Err(format!("{option} requires {expectation}"));
    }
    Ok(value)
}

fn set_project_path(target: &mut Option<String>, value: &str) -> Result<(), String> {
    if target.is_some() {
        return Err(format!("unexpected positional argument: {value}"));
    }
    *target = Some(value.to_string());
    Ok(())
}

fn validate_log_component(value: &str) -> Result<(), String> {
    match value {
        "index" | "daemon" | "mcp" | "telemetry" => Ok(()),
        _ => Err("--component requires index, daemon, mcp, or telemetry".to_string()),
    }
}

fn repository_root(current_dir: &Path, project_path: Option<&str>) -> String {
    let raw = Path::new(project_path.unwrap_or("."));
    let path = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        current_dir.join(raw)
    };
    path.display().to_string()
}

fn state_dir_override<F>(env_lookup: &F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    env_lookup("REPOGRAMMAR_DIR")
}

fn init_outcome_human(outcome: &RepositoryInitOutcome) -> String {
    let mut output = format!(
        "init: repository-local state ready\nstate_dir: {}\ncreated: {}\ngit_info_exclude: {}\nroot_gitignore: {}\nstorage: not_implemented\nindexing: not_implemented\n",
        outcome.state_dir,
        outcome.created,
        if outcome.git_info_exclude_updated {
            "updated"
        } else {
            "already_present_or_not_applicable"
        },
        if outcome.root_gitignore_updated {
            "updated"
        } else {
            "not_modified"
        }
    );
    for entry in &outcome.repaired_entries {
        output.push_str("repaired_entry: ");
        output.push_str(entry);
        output.push('\n');
    }
    output
}

fn init_outcome_json(outcome: &RepositoryInitOutcome) -> String {
    format!(
        "{{\"command\":\"init\",\"status\":\"initialized\",\"state_dir\":\"{}\",\"created\":{},\"git_info_exclude_updated\":{},\"root_gitignore_updated\":{},\"storage\":\"not_implemented\",\"indexing\":\"not_implemented\",\"repaired_entries\":{}}}\n",
        json_string(&outcome.state_dir),
        outcome.created,
        outcome.git_info_exclude_updated,
        outcome.root_gitignore_updated,
        json_array(&outcome.repaired_entries)
    )
}

fn uninit_outcome_human(outcome: &RepositoryUninitOutcome) -> String {
    format!(
        "uninit: repository-local state {}\nstate_dir: {}\nlogs: removed with state dir when present\n",
        if outcome.removed {
            "removed"
        } else {
            "was not present"
        },
        outcome.state_dir
    )
}

fn uninit_outcome_json(outcome: &RepositoryUninitOutcome) -> String {
    format!(
        "{{\"command\":\"uninit\",\"state_dir\":\"{}\",\"removed\":{},\"logs_removed\":{}}}\n",
        json_string(&outcome.state_dir),
        outcome.removed,
        outcome.removed
    )
}

fn index_outcome_human(
    command: &str,
    outcome: &IndexingOutcome,
    options: &LifecycleOptions,
) -> String {
    let mut output = format!(
        "{command}: syntax-only code units stored\nactive_generation: {}\ndiscovered_files: {}\nstored_files: {}\nskipped_paths: {}\nindexed_units: {}\nindexing: syntax_only_code_units\nparser: syntax_only\nsemantic_worker: deferred\nmining: deferred\nprogress: {}\n",
        outcome.active_generation.as_deref().unwrap_or("none"),
        outcome.discovered_files,
        outcome.discovered_files,
        outcome.skipped_paths,
        outcome.indexed_units,
        options.progress.as_str()
    );
    for warning in &outcome.warnings {
        output.push_str("warning: ");
        output.push_str(warning);
        output.push('\n');
    }
    output
}

fn index_outcome_json(
    command: &str,
    outcome: &IndexingOutcome,
    options: &LifecycleOptions,
) -> String {
    format!(
        "{{\"command\":\"{}\",\"status\":\"complete\",\"generation_id\":{},\"discovered_files\":{},\"stored_files\":{},\"skipped_paths\":{},\"indexed_units\":{},\"indexing\":\"syntax_only_code_units\",\"parser\":\"syntax_only\",\"semantic_worker\":\"deferred\",\"mining\":\"deferred\",\"progress\":\"{}\",\"warnings\":{}}}\n",
        json_string(command),
        optional_json_string(outcome.active_generation.as_deref()),
        outcome.discovered_files,
        outcome.discovered_files,
        outcome.skipped_paths,
        outcome.indexed_units,
        options.progress.as_str(),
        json_array(&outcome.warnings)
    )
}

fn status_human(report: &RepositoryStatusReport) -> String {
    let mut output = String::new();
    output.push_str(report.status.as_human_message());
    output.push('\n');
    output.push_str(&format!("state_dir: {}\n", report.state_dir));
    output.push_str(&format!("manifest: {}\n", manifest_status(report.manifest)));
    output.push_str(&format!(
        "schema_version: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.schema_version)
            .map(|version| version.to_string())
            .unwrap_or_else(|| {
                if report.storage != RepositoryImplementationStatus::Unhealthy
                    && matches!(report.manifest, RepositoryManifestStatus::Valid)
                {
                    "1".to_string()
                } else {
                    "none".to_string()
                }
            })
    ));
    output.push_str(&format!(
        "journal_mode: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.journal_mode.as_deref())
            .unwrap_or("not_implemented")
    ));
    output.push_str(&format!(
        "integrity_check: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.integrity_check.as_deref())
            .unwrap_or("not_implemented")
    ));
    output.push_str(&format!(
        "active_generation: {}\n",
        match &report.status {
            RepositoryStatus::Initialized { active_generation } => active_generation.as_str(),
            _ => "none",
        }
    ));
    output.push_str(&format!(
        "storage: {}\n",
        implementation_status(report.storage)
    ));
    output.push_str(&format!(
        "indexing: {}\n",
        implementation_status(report.indexing)
    ));
    if let Some(error) = &report.storage_error {
        output.push_str("storage_error: ");
        output.push_str(error);
        output.push('\n');
    }
    for subdir in &report.missing_subdirs {
        output.push_str("missing_subdir: ");
        output.push_str(subdir);
        output.push('\n');
    }
    output
}

fn status_json(report: &RepositoryStatusReport) -> String {
    format!(
        "{{\"command\":\"status\",\"initialized\":{},\"state_dir\":\"{}\",\"status\":\"{}\",\"manifest\":\"{}\",\"active_generation\":{},\"schema_version\":{},\"journal_mode\":{},\"integrity_check\":{},\"foreign_keys_enabled\":{},\"storage\":\"{}\",\"indexing\":\"{}\",\"storage_error\":{},\"missing_subdirs\":{}}}\n",
        matches!(report.status, RepositoryStatus::Initialized { .. }),
        json_string(&report.state_dir),
        repository_status_value(&report.status),
        manifest_status(report.manifest),
        match &report.status {
            RepositoryStatus::Initialized { active_generation }
                if active_generation != "none" && active_generation != "not implemented" =>
            {
                optional_json_string(Some(active_generation.as_str()))
            }
            _ => "null".to_string(),
        },
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.schema_version)
            .map(|version| version.to_string())
            .unwrap_or_else(|| {
                if report.storage != RepositoryImplementationStatus::Unhealthy
                    && matches!(report.manifest, RepositoryManifestStatus::Valid)
                {
                    "1".to_string()
                } else {
                    "null".to_string()
                }
            }),
        optional_json_string(
            report
                .storage_inspection
                .as_ref()
                .and_then(|inspection| inspection.journal_mode.as_deref())
        ),
        optional_json_string(
            report
                .storage_inspection
                .as_ref()
                .and_then(|inspection| inspection.integrity_check.as_deref())
        ),
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.foreign_keys_enabled)
            .map(|enabled| enabled.to_string())
            .unwrap_or_else(|| "null".to_string()),
        implementation_status(report.storage),
        implementation_status(report.indexing),
        optional_json_string(report.storage_error.as_deref()),
        json_array(&report.missing_subdirs)
    )
}

fn doctor_human(report: &RepositoryDoctorReport) -> String {
    let mut output = String::from("doctor: repository lifecycle diagnostics\n");
    output.push_str(&format!("state_dir: {}\n", report.status.state_dir));
    output.push_str(&format!(
        "status: {}\n",
        repository_status_value(&report.status.status)
    ));
    for finding in &report.findings {
        output.push_str(&format!(
            "{}: {} ({})\n",
            doctor_severity(finding.severity),
            doctor_code(finding.code),
            finding.detail
        ));
    }
    output
}

fn doctor_json(report: &RepositoryDoctorReport) -> String {
    let findings = report
        .findings
        .iter()
        .map(finding_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"command\":\"doctor\",\"initialized\":{},\"state_dir\":\"{}\",\"status\":\"{}\",\"checks\":{{\"manifest\":\"{}\",\"required_subdirectories\":\"{}\",\"storage\":\"{}\",\"indexing\":\"{}\",\"schema_version\":{},\"journal_mode\":{},\"integrity_check\":{}}},\"findings\":[{}]}}\n",
        matches!(report.status.status, RepositoryStatus::Initialized { .. }),
        json_string(&report.status.state_dir),
        repository_status_value(&report.status.status),
        manifest_status(report.status.manifest),
        if matches!(report.status.status, RepositoryStatus::NotInitialized) {
            "not_applicable"
        } else if report.status.missing_subdirs.is_empty() {
            "pass"
        } else {
            "fail"
        },
        implementation_status(report.status.storage),
        implementation_status(report.status.indexing),
        report
            .status
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.schema_version)
            .map(|version| version.to_string())
            .unwrap_or_else(|| "null".to_string()),
        optional_json_string(
            report
                .status
                .storage_inspection
                .as_ref()
                .and_then(|inspection| inspection.journal_mode.as_deref())
        ),
        optional_json_string(
            report
                .status
                .storage_inspection
                .as_ref()
                .and_then(|inspection| inspection.integrity_check.as_deref())
        ),
        findings
    )
}

fn unlock_human(outcome: &RepositoryUnlockReport) -> String {
    let mut output = format!(
        "unlock: {}\nremoved_locks: {}\n",
        outcome.message, outcome.removed_locks
    );
    for lock in &outcome.inspected_locks {
        output.push_str("inspected_lock: ");
        output.push_str(lock);
        output.push('\n');
    }
    output
}

fn unlock_json(outcome: &RepositoryUnlockReport) -> String {
    format!(
        "{{\"command\":\"unlock\",\"state_dir\":\"{}\",\"removed_locks\":{},\"inspected_locks\":{},\"message\":\"{}\"}}\n",
        json_string(&outcome.state_dir),
        outcome.removed_locks,
        json_array(&outcome.inspected_locks),
        json_string(&outcome.message)
    )
}

fn logs_human(outcome: &RepositoryLogsReport) -> String {
    let mut output = format!(
        "logs: {}\nstate_dir: {}\navailable: {}\nredacted: {}\n",
        outcome.message, outcome.state_dir, outcome.available, outcome.redacted
    );
    output.push_str(&format!("entries: {}\n", outcome.entries.len()));
    output
}

fn logs_json(outcome: &RepositoryLogsReport, options: &LogsOptions) -> String {
    format!(
        "{{\"command\":\"logs\",\"state_dir\":\"{}\",\"available\":{},\"redacted\":{},\"paths\":\"repo_relative_only\",\"component_filter\":{},\"tail\":{},\"since\":{},\"entries\":{},\"message\":\"{}\"}}\n",
        json_string(&outcome.state_dir),
        outcome.available,
        outcome.redacted,
        optional_json_string(options.component.as_deref()),
        options
            .tail
            .map(|tail| tail.to_string())
            .unwrap_or_else(|| "null".to_string()),
        optional_json_string(options.since.as_deref()),
        json_array(&outcome.entries),
        json_string(&outcome.message)
    )
}

fn repository_status_value(status: &RepositoryStatus) -> &'static str {
    match status {
        RepositoryStatus::NotInitialized => "not_initialized",
        RepositoryStatus::Initialized { .. } => "initialized",
        RepositoryStatus::CorruptedManifest => "corrupted_manifest",
    }
}

fn manifest_status(status: RepositoryManifestStatus) -> &'static str {
    match status {
        RepositoryManifestStatus::Missing => "missing",
        RepositoryManifestStatus::Valid => "valid",
        RepositoryManifestStatus::Corrupted => "corrupted",
    }
}

fn implementation_status(status: RepositoryImplementationStatus) -> &'static str {
    match status {
        RepositoryImplementationStatus::NotImplemented => "not_implemented",
        RepositoryImplementationStatus::Available => "available",
        RepositoryImplementationStatus::FileManifestOnly => "file_manifest_only",
        RepositoryImplementationStatus::SyntaxOnlyCodeUnits => "syntax_only_code_units",
        RepositoryImplementationStatus::Unhealthy => "unhealthy",
    }
}

fn doctor_severity(severity: RepositoryDoctorSeverity) -> &'static str {
    match severity {
        RepositoryDoctorSeverity::Info => "info",
        RepositoryDoctorSeverity::Warning => "warning",
        RepositoryDoctorSeverity::Error => "error",
    }
}

fn doctor_code(code: RepositoryDoctorCode) -> &'static str {
    match code {
        RepositoryDoctorCode::NotInitialized => "NOT_INITIALIZED",
        RepositoryDoctorCode::CorruptedManifest => "CORRUPTED_MANIFEST",
        RepositoryDoctorCode::MissingSubdir => "MISSING_SUBDIR",
        RepositoryDoctorCode::StorageNotImplemented => "STORAGE_NOT_IMPLEMENTED",
        RepositoryDoctorCode::StorageReady => "STORAGE_READY",
        RepositoryDoctorCode::StorageInvalid => "STORAGE_INVALID",
        RepositoryDoctorCode::StorageNoActiveGeneration => "STORAGE_NO_ACTIVE_GENERATION",
        RepositoryDoctorCode::IndexingNotImplemented => "INDEXING_NOT_IMPLEMENTED",
        RepositoryDoctorCode::IndexingFileManifestOnly => "INDEXING_FILE_MANIFEST_ONLY",
        RepositoryDoctorCode::IndexingSyntaxOnlyCodeUnits => "INDEXING_SYNTAX_ONLY_CODE_UNITS",
    }
}

fn finding_json(finding: &RepositoryDoctorFinding) -> String {
    format!(
        "{{\"severity\":\"{}\",\"code\":\"{}\",\"detail\":\"{}\"}}",
        doctor_severity(finding.severity),
        doctor_code(finding.code),
        json_string(&finding.detail)
    )
}

fn lifecycle_error(command: &str, json: bool, error: RepoGrammarError) -> CliOutput {
    if json {
        CliOutput::failure(
            2,
            format!(
                "{{\"command\":\"{}\",\"status\":\"error\",\"reason\":\"{}\"}}\n",
                json_string(command),
                json_string(&error.to_string())
            ),
        )
    } else {
        CliOutput::failure(2, format!("{error}\n"))
    }
}

fn json_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", json_string(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn optional_json_string(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn json_string(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if (control as u32) <= 0x1f => {
                escaped.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::discovery::FilesystemFileDiscovery;
    use crate::adapters::filesystem::source_store::FilesystemSourceStore;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::application::indexing::{
        index_repository_with_discovery_parser_and_store, IndexingRequest,
    };
    use crate::application::repository::DEFAULT_STATE_DIR;
    use crate::application::repository::{
        repository_doctor_with_storage, repository_state_location, repository_status_with_storage,
    };
    use crate::ports::index_store::STORAGE_SCHEMA_VERSION;
    use crate::test_support::TempWorkspace;
    use rusqlite::Connection;
    use serde_json::Value;
    use std::fs;

    struct TestRuntime;

    impl TestRuntime {
        fn store_for_status_request(
            &self,
            request: &RepositoryStatusRequest,
        ) -> Result<SqliteIndexStore, RepoGrammarError> {
            let location = repository_state_location(request.clone())?;
            Ok(SqliteIndexStore::new(location.state_dir))
        }
    }

    impl CliRuntime for TestRuntime {
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

            index_repository_with_discovery_parser_and_store(
                IndexingRequest {
                    repository_root: request.repository_root,
                    max_file_bytes: request.max_file_bytes,
                },
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
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

    fn indexed_paths(workspace: &TempWorkspace, generation_id: &str) -> Vec<String> {
        let database = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("generations")
            .join(generation_id)
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        let paths = connection
            .prepare("SELECT path FROM indexed_files ORDER BY path")
            .expect("prepare indexed paths")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query indexed paths")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect indexed paths");
        paths
    }

    #[test]
    fn version_succeeds() {
        let output = run(["--version"]);

        assert_eq!(output.status, 0);
        assert!(output.stdout.starts_with("repogrammar "));
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn pattern_family_command_surface_is_recognized() {
        let workspace = TempWorkspace::new("cli-query-surface");
        let env = |_: &str| None;
        for command in [
            "find", "families", "family", "member", "explain", "check", "files", "units",
        ] {
            let output = run_with_context([command], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stderr.starts_with(
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init\n"
            ));
            assert!(output.stderr.contains("not implemented yet"));
            assert!(output.stdout.is_empty());
        }
    }

    #[test]
    fn query_options_are_accepted() {
        let workspace = TempWorkspace::new("cli-query-options");
        let env = |_: &str| None;
        let output = run_with_context(
            [
                "find",
                "--project",
                ".",
                "--token-budget",
                "8000",
                "--json",
                "--include-variations",
                "--include-exceptions",
                "src/user.ts",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository is not initialized");
        assert_eq!(fallback["guidance"], "run repogrammar init");
        assert_eq!(fallback["command"], "find");
        assert_eq!(fallback["implemented"], false);
    }

    #[test]
    fn every_query_command_supports_json_missing_index_fallback() {
        let workspace = TempWorkspace::new("cli-query-missing-index");
        let env = |_: &str| None;
        for command in [
            "find", "families", "family", "member", "explain", "check", "files", "units",
        ] {
            let output = run_with_context([command, "--json"], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
            let fallback: Value =
                serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
            assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
            assert_eq!(fallback["reason"], "repository is not initialized");
            assert_eq!(fallback["guidance"], "run repogrammar init");
            assert_eq!(fallback["command"], command);
            assert_eq!(fallback["implemented"], false);
        }
    }

    #[test]
    fn query_fallback_distinguishes_initialized_state_from_missing_state() {
        let workspace = TempWorkspace::new("cli-query-initialized-fallback");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output =
            run_with_context_and_runtime(["find", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(
            fallback["reason"],
            "query execution requires pattern-family evidence"
        );
        assert_eq!(
            fallback["guidance"],
            "run repogrammar index after pattern-family indexing is implemented"
        );
        assert_eq!(fallback["implemented"], false);
    }

    #[test]
    fn query_fallback_reports_unavailable_repository_status() {
        let workspace = TempWorkspace::new("cli-query-corrupted-state");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            "broken",
        )
        .expect("corrupt manifest");

        let output = run_with_context(["find", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 2);
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository status is unavailable");
        assert_eq!(fallback["guidance"], "run repogrammar doctor");
        assert_eq!(fallback["implemented"], false);
    }

    #[test]
    fn forbidden_graph_commands_are_not_top_level() {
        for command in [
            "callers", "callees", "impact", "affected", "node", "explore",
        ] {
            let output = run([command]);

            assert_eq!(output.status, 2);
            assert!(output.stderr.contains("not a v0.1 top-level command"));
        }
    }

    #[test]
    fn init_creates_state_and_json_is_parseable() {
        let workspace = TempWorkspace::new("cli-init");
        create_git_dir(workspace.path());
        let env = |_: &str| None;

        let output = run_with_context(
            ["init", "--json", "--write-gitignore"],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "initialized");
        assert_eq!(value["state_dir"], DEFAULT_STATE_DIR);
        assert_eq!(value["storage"], "not_implemented");
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
        assert!(workspace.path().join(".gitignore").is_file());
    }

    #[test]
    fn init_human_output_mentions_deferred_storage_without_claiming_indexing() {
        let workspace = TempWorkspace::new("cli-init-human");
        let env = |_: &str| None;

        let output = run_with_context(["init"], workspace.path(), &env);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("repository-local state ready"));
        assert!(output.stdout.contains("storage: not_implemented"));
        assert!(output.stdout.contains("indexing: not_implemented"));
    }

    #[test]
    fn status_reports_not_initialized_and_initialized_in_human_and_json() {
        let workspace = TempWorkspace::new("cli-status");
        let env = |_: &str| None;

        let human = run_with_context(["status"], workspace.path(), &env);
        assert_eq!(human.status, 0);
        assert!(human
            .stdout
            .contains("RepoGrammar repository status: not initialized"));

        let json = run_with_context(["status", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(json.stdout.trim()).expect("status JSON");
        assert_eq!(value["initialized"], false);
        assert_eq!(value["state_dir"], DEFAULT_STATE_DIR);

        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let initialized = run_with_context(["status", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(initialized.stdout.trim()).expect("status JSON");
        assert_eq!(value["initialized"], true);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["indexing"], "not_implemented");
    }

    #[test]
    fn doctor_handles_valid_and_corrupted_state() {
        let workspace = TempWorkspace::new("cli-doctor");
        let env = |_: &str| None;

        let missing = run_with_context(["doctor", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(missing.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["initialized"], false);
        assert_eq!(value["checks"]["storage"], "not_implemented");
        assert_eq!(value["checks"]["required_subdirectories"], "not_applicable");

        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            "broken",
        )
        .expect("corrupt manifest");
        let corrupt = run_with_context(["doctor", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(corrupt.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["manifest"], "corrupted");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "CORRUPTED_MANIFEST"));
    }

    #[test]
    fn index_json_stores_syntax_only_code_units_without_family_claims() {
        let workspace = TempWorkspace::new("cli-index-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        fs::write(workspace.path().join("note.md"), "# ignored\n").expect("write ignored");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["command"], "index");
        assert_eq!(value["status"], "complete");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["discovered_files"], 1);
        assert_eq!(value["stored_files"], 1);
        assert!(value["indexed_units"].as_u64().expect("indexed unit count") >= 1);
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["parser"], "syntax_only");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["mining"], "deferred");
        assert!(workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .is_file());
    }

    #[test]
    fn sync_json_rebuilds_a_new_file_manifest_generation() {
        let workspace = TempWorkspace::new("cli-sync-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace.path().join("b.tsx"),
            "export function B(){ return null; }\n",
        )
        .expect("write b");

        let output =
            run_with_context_and_runtime(["sync", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("sync JSON");
        assert_eq!(value["command"], "sync");
        assert_eq!(value["generation_id"], "gen-000002");
        assert_eq!(value["discovered_files"], 2);
        assert_eq!(value["stored_files"], 2);
        assert!(value["indexed_units"].as_u64().expect("indexed unit count") >= 2);
        assert_eq!(
            fs::read_to_string(
                workspace
                    .path()
                    .join(DEFAULT_STATE_DIR)
                    .join("current-generation")
            )
            .expect("read current generation")
            .trim(),
            "gen-000002"
        );
        assert_eq!(indexed_paths(&workspace, "gen-000001"), vec!["a.ts"]);
        assert_eq!(
            indexed_paths(&workspace, "gen-000002"),
            vec!["a.ts", "b.tsx"]
        );

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000002");
    }

    #[test]
    fn index_refuses_uninitialized_repository() {
        let workspace = TempWorkspace::new("cli-index-uninitialized");
        let env = |_: &str| None;
        let runtime = TestRuntime;

        let output =
            run_with_context_and_runtime(["index", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("not initialized"));
    }

    #[test]
    fn index_refuses_corrupted_manifest_with_json() {
        let workspace = TempWorkspace::new("cli-index-corrupt-manifest");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            "broken",
        )
        .expect("corrupt manifest");

        let output =
            run_with_context_and_runtime(["index", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("corrupted"));
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
    }

    #[test]
    fn index_refuses_missing_state_subdirectories_without_recreating_them() {
        let workspace = TempWorkspace::new("cli-index-missing-subdir");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let generations = workspace.path().join(DEFAULT_STATE_DIR).join("generations");
        fs::remove_dir_all(&generations).expect("remove generations");

        let output =
            run_with_context_and_runtime(["index", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("missing required subdirectories"));
        assert!(!generations.exists());
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
    }

    #[test]
    fn status_and_doctor_report_no_active_generation_before_index() {
        let workspace = TempWorkspace::new("cli-storage-no-active");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "not_implemented");

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["indexing"], "not_implemented");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_NO_ACTIVE_GENERATION"));
    }

    #[test]
    fn doctor_json_reports_missing_subdir_and_storage_invalid() {
        let workspace = TempWorkspace::new("cli-doctor-missing-subdir-storage");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let generations = workspace.path().join(DEFAULT_STATE_DIR).join("generations");
        fs::remove_dir_all(&generations).expect("remove generations");

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert_eq!(value["schema_version"], Value::Null);
        assert_eq!(value["journal_mode"], Value::Null);
        assert_eq!(value["integrity_check"], Value::Null);
        assert_eq!(value["storage"], "unhealthy");
        assert!(value["storage_error"]
            .as_str()
            .expect("storage error")
            .contains("generations"));

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["required_subdirectories"], "fail");
        assert_eq!(value["checks"]["storage"], "unhealthy");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "MISSING_SUBDIR"));
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_INVALID"));
        assert!(!generations.exists());
    }

    #[test]
    fn index_human_reports_syntax_only_without_family_claims() {
        let workspace = TempWorkspace::new("cli-index-human-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let output = run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("syntax-only code units stored"));
        assert!(output.stdout.contains("indexed_units: 1"));
        assert!(output.stdout.contains("indexing: syntax_only_code_units"));
        assert!(output.stdout.contains("parser: syntax_only"));
        assert!(output.stdout.contains("semantic_worker: deferred"));
        assert!(output.stdout.contains("mining: deferred"));
        assert!(!output.stdout.contains("DOMINANT_PATTERN"));
        assert!(!output.stdout.contains("CONFORMS"));
        assert!(!output.stdout.contains("pattern family"));
    }

    #[test]
    fn status_and_doctor_report_storage_health_after_index() {
        let workspace = TempWorkspace::new("cli-storage-health");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.js"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["schema_version"], STORAGE_SCHEMA_VERSION);
        assert_eq!(value["journal_mode"], "wal");
        assert_eq!(value["integrity_check"], "ok");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "syntax_only_code_units");

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["indexing"], "syntax_only_code_units");
        assert_eq!(value["checks"]["integrity_check"], "ok");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "INDEXING_SYNTAX_ONLY_CODE_UNITS"));
    }

    #[test]
    fn doctor_reports_broken_active_generation_pointer_without_panic() {
        let workspace = TempWorkspace::new("cli-storage-broken-pointer");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("current-generation"),
            "gen-999999\n",
        )
        .expect("break pointer");

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(doctor.status, 0);
        let doctor_value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(doctor_value["checks"]["storage"], "unhealthy");
        assert!(doctor_value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_INVALID"));

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert_eq!(value["schema_version"], Value::Null);
        assert_eq!(value["storage"], "unhealthy");

        let index =
            run_with_context_and_runtime(["sync", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(index.status, 2);
        let value: Value = serde_json::from_str(index.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "sync");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("storage is unhealthy"));
        assert_eq!(
            fs::read_to_string(
                workspace
                    .path()
                    .join(DEFAULT_STATE_DIR)
                    .join("current-generation")
            )
            .expect("read broken pointer")
            .trim(),
            "gen-999999"
        );
    }

    #[test]
    fn uninit_requires_yes_and_removes_state_only() {
        let workspace = TempWorkspace::new("cli-uninit");
        let env = |_: &str| None;
        fs::write(
            workspace.path().join("business.ts"),
            "export const x = 1;\n",
        )
        .expect("write business source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let missing_yes = run_with_context(["uninit"], workspace.path(), &env);
        assert_eq!(missing_yes.status, 2);
        assert!(missing_yes
            .stderr
            .contains("requires explicit confirmation"));

        let output = run_with_context(["uninit", "--yes", "--json"], workspace.path(), &env);
        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("uninit JSON");
        assert_eq!(value["removed"], true);
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
        assert!(workspace.path().join("business.ts").is_file());
    }

    #[test]
    fn repogrammar_dir_override_is_used_and_unsafe_values_are_rejected() {
        let workspace = TempWorkspace::new("cli-env-state-dir");
        let safe_env =
            |key: &str| (key == "REPOGRAMMAR_DIR").then(|| ".repogrammar-linux".to_string());

        let output = run_with_context(["init", "--json"], workspace.path(), &safe_env);
        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["state_dir"], ".repogrammar-linux");
        assert!(workspace.path().join(".repogrammar-linux").is_dir());

        let unsafe_env = |key: &str| (key == "REPOGRAMMAR_DIR").then(|| "../outside".to_string());
        let error = run_with_context(["status", "--json"], workspace.path(), &unsafe_env);
        assert_eq!(error.status, 2);
        assert!(error.stderr.contains("repository state directory override"));
    }

    #[test]
    fn unlock_does_not_blind_delete_locks() {
        let workspace = TempWorkspace::new("cli-unlock");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let lock_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks/index.lock");
        fs::write(&lock_path, "{}").expect("write lock");

        let output = run_with_context(["unlock", "--json"], workspace.path(), &env);
        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("unlock JSON");
        assert_eq!(value["removed_locks"], 0);
        assert_eq!(value["inspected_locks"][0], "index.lock");
        assert!(lock_path.exists());

        let force = run_with_context(["unlock", "--force"], workspace.path(), &env);
        assert_eq!(force.status, 2);
        assert!(force.stderr.contains("--force requires --yes"));
    }

    #[test]
    fn logs_json_is_redacted_metadata_and_omits_absolute_paths() {
        let workspace = TempWorkspace::new("cli-logs");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("logs/index.log"),
            format!("absolute path would be {}\n", workspace.path().display()),
        )
        .expect("write log");

        let output = run_with_context(
            [
                "logs",
                "--json",
                "--component",
                "index",
                "--since",
                "1h",
                "--tail",
                "20",
                "--redact",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("logs JSON");
        assert_eq!(value["command"], "logs");
        assert_eq!(value["paths"], "repo_relative_only");
        assert_eq!(value["redacted"], true);
        assert_eq!(value["component_filter"], "index");
        assert!(value["entries"].as_array().expect("entries").is_empty());
    }

    #[test]
    fn unknown_lifecycle_options_are_rejected() {
        let status = run(["status", "--write-gitignore"]);
        assert_eq!(status.status, 2);
        assert!(status
            .stderr
            .contains("unknown status option: --write-gitignore"));

        let logs = run(["logs", "--mystery"]);
        assert_eq!(logs.status, 2);
        assert!(logs.stderr.contains("unknown logs option: --mystery"));
    }

    #[test]
    fn install_dry_run_accepts_required_flags() {
        let output = run([
            "install",
            "--target",
            "codex",
            "--scope",
            "project",
            "--dry-run",
            "--yes",
            "--print-config",
            "--no-telemetry",
            "--no-permissions",
        ]);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("target=codex"));
        assert!(output.stdout.contains("telemetry=off"));
    }

    #[test]
    fn status_doctor_stats_and_telemetry_status_are_safe() {
        assert_eq!(run(["status"]).status, 0);
        assert_eq!(run(["doctor"]).status, 0);
        assert_eq!(run(["stats"]).status, 0);
        assert_eq!(run(["telemetry", "status"]).status, 0);
    }

    fn create_git_dir(root: &Path) {
        fs::create_dir(root.join(".git")).expect("create .git");
        fs::create_dir(root.join(".git/info")).expect("create .git/info");
    }
}
