use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT_GUIDES: &[&str] = &["AGENTS.md", "CLAUDE.md"];
const REQUIRED_SKILLS: &[&str] = &[
    "agent-integration",
    "implement-change",
    "documentation-sync",
    "rust-quality",
    "major-feature-workflow",
    "repogrammar-domain",
    "repogrammar-cli",
    "mcp-contract-change",
    "telemetry-and-metrics",
];
const REQUIRED_DOCUMENTS: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "CHANGELOG.md",
    "docs/README.md",
    "docs/architecture/overview.md",
    "docs/architecture/dependency-rules.md",
    "docs/architecture/module-map.md",
    "docs/specifications/cli.md",
    "docs/specifications/installation.md",
    "docs/specifications/initialization-progress.md",
    "docs/specifications/product.md",
    "docs/specifications/domain-model.md",
    "docs/specifications/indexing-pipeline.md",
    "docs/specifications/metrics.md",
    "docs/specifications/storage.md",
    "docs/specifications/telemetry.md",
    "docs/specifications/mcp-api.md",
    "docs/specifications/unknowns.md",
    "docs/specifications/python-analysis.md",
    "docs/plans/v0.1-parallel-development-plan.md",
    "docs/plans/python-dogfooding-plan.md",
    "docs/plans/python-v0.1-implementation-plan.md",
    "docs/plans/codegraph-provider-plan.md",
    "docs/plans/v0.1-substrate-hardening-checkpoint.md",
    "docs/plans/top-20-language-expansion-plan.md",
    "docs/development/agent-workflow.md",
    "docs/development/branching-and-commits.md",
    "docs/development/documentation-policy.md",
    "docs/development/repository-guard.md",
    "docs/development/testing.md",
    "docs/decisions/README.md",
    "docs/decisions/ADR-0001-rust-core.md",
    "docs/decisions/ADR-0002-local-sqlite-index.md",
    "docs/decisions/ADR-0003-pattern-family-model.md",
    "docs/decisions/ADR-0004-rust-core-language-native-workers.md",
    "docs/decisions/ADR-0005-ts-js-first-mvp.md",
    "docs/decisions/ADR-0006-pattern-family-cli.md",
    "docs/decisions/ADR-0007-safe-install-progress-telemetry.md",
    "docs/decisions/ADR-0008-repo-local-state-boundary.md",
    "docs/decisions/ADR-0009-experimental-python-dogfooding.md",
    "docs/decisions/ADR-0010-optional-codegraph-provider.md",
    "docs/decisions/ADR-0011-python-first-v0-1.md",
    "docs/decisions/ADR-0012-python-selective-analysis-cascade.md",
    "docs/decisions/ADR-0020-top-20-language-expansion-gate.md",
    "docs/roadmap.md",
    ".agents/memories/README.md",
    ".agents/memories/project-state.md",
    ".agents/memories/known-constraints.md",
    ".agents/memories/open-questions.md",
    ".agents/memories/v0.1-parallel-development-plan.md",
    ".agents/memories/python-dogfooding-plan.md",
    ".agents/memories/python-v0.1-algorithm-plan.md",
    ".agents/memories/codegraph-provider-plan.md",
    ".agents/memories/unknown-governance.md",
    ".agents/memories/v0.1-substrate-hardening-checkpoint.md",
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
    ".github/workflows/npm-tag-reconcile.yml",
    "docs/specifications/semantic-workers.md",
    "src/rust/bin/repo_guard.rs",
];
const DEPRECATED_NODE20_ACTIONS: &[(&str, &str)] = &[
    ("actions/checkout@v4", "use actions/checkout@v5 or newer"),
    (
        "actions/setup-node@v4",
        "use actions/setup-node@v5 or newer",
    ),
];
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "c", "cc", "cpp", "cxx", "h", "hpp", "hh", "hxx", "go", "py", "js", "jsx", "ts", "tsx",
    "java", "cs", "kt", "kts", "sh", "bash", "zsh", "ps1", "sql",
];
const IGNORED_DIRS: &[&str] = &[".git", "target", ".codegraph", ".repogrammar"];
const IGNORED_DIR_PREFIXES: &[&str] = &[".repogrammar-"];
const MAX_GITDIR_POINTER_BYTES: u64 = 4 * 1024;

fn main() {
    let root = match env::current_dir() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            std::process::exit(1);
        }
    };
    let result = run(env::args().skip(1), &root);
    write_std_streams(&result.stdout, &result.stderr);
    std::process::exit(result.status);
}

/// Writes captured stdout/stderr, tolerating a broken pipe (e.g. piping to
/// `head`) so the process exits with its intended status instead of panicking
/// in the `print!`/`eprint!` macros.
fn write_std_streams(stdout_text: &str, stderr_text: &str) {
    use std::io::Write;
    let mut out = io::stdout().lock();
    let _ = out.write_all(stdout_text.as_bytes());
    let _ = out.flush();
    let mut err = io::stderr().lock();
    let _ = err.write_all(stderr_text.as_bytes());
    let _ = err.flush();
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandResult {
    status: i32,
    stdout: String,
    stderr: String,
}

impl CommandResult {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            stdout: message.into(),
            stderr: String::new(),
        }
    }

    fn err(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            stdout: String::new(),
            stderr: message.into(),
        }
    }
}

fn run<I, S>(args: I, root: &Path) -> CommandResult
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    match args.as_slice() {
        [command] if command == "check" => run_check(root),
        [command, flag, source] if command == "sync-agent-guides" && flag == "--from" => {
            match sync_agent_guides(root, source) {
                Ok(()) => CommandResult::ok("agent guides synchronized\n"),
                Err(error) => CommandResult::err(format!("{error}\n")),
            }
        }
        [command, base_flag, base, head_flag, head]
            if command == "check-diff" && base_flag == "--base" && head_flag == "--head" =>
        {
            match changed_paths_between(root, base, head).and_then(|paths| check_diff_paths(&paths))
            {
                Ok(()) => CommandResult::ok("diff documentation gate passed\n"),
                Err(error) => CommandResult::err(format!("{error}\n")),
            }
        }
        [command, binary_flag, binary, worker_flag, worker, fixture_flag, fixture, version_flag, expected_version]
            if command == "smoke-packaged-artifact"
                && binary_flag == "--binary"
                && worker_flag == "--worker"
                && fixture_flag == "--fixture"
                && version_flag == "--expected-version" =>
        {
            match smoke_packaged_artifact(root, binary, worker, fixture, expected_version) {
                Ok(()) => CommandResult::ok("packaged artifact smoke passed\n"),
                Err(error) => {
                    CommandResult::err(format!("packaged artifact smoke failed: {error}\n"))
                }
            }
        }
        [command, version_flag, version, preview_flag, preview, latest_flag, latest]
            if command == "preview-dist-tag-action"
                && version_flag == "--version"
                && preview_flag == "--preview"
                && latest_flag == "--latest" =>
        {
            match preview_dist_tag_action(version, preview, latest) {
                Ok(action) => CommandResult::ok(format!("{}\n", action.as_str())),
                Err(error) => {
                    CommandResult::err(format!("preview dist-tag classification failed: {error}\n"))
                }
            }
        }
        [] => CommandResult::err(format!("{}\n", usage())),
        _ => CommandResult::err(format!("unknown or invalid arguments\n{}\n", usage())),
    }
}

fn usage() -> &'static str {
    "Usage: repo-guard check | sync-agent-guides --from <AGENTS.md|CLAUDE.md> | check-diff --base <rev> --head <rev> | smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version> | preview-dist-tag-action --version <version> --preview <version> --latest <version-or-empty>"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewDistTagAction {
    NoTag,
    RemovePrerelease,
    PreserveStable,
}

impl PreviewDistTagAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoTag => "no_latest",
            Self::RemovePrerelease => "remove_prerelease_latest",
            Self::PreserveStable => "preserve_stable_latest",
        }
    }
}

fn preview_dist_tag_action(
    version: &str,
    preview: &str,
    latest: &str,
) -> Result<PreviewDistTagAction, &'static str> {
    if !is_bounded_version(version) || !has_prerelease(version) {
        return Err("manifest version is not a bounded prerelease");
    }
    if preview != version {
        return Err("preview does not match the manifest version");
    }
    if latest.is_empty() {
        return Ok(PreviewDistTagAction::NoTag);
    }
    if !is_bounded_version(latest) {
        return Err("latest is malformed");
    }
    if has_prerelease(latest) {
        Ok(PreviewDistTagAction::RemovePrerelease)
    } else {
        Ok(PreviewDistTagAction::PreserveStable)
    }
}

fn has_prerelease(version: &str) -> bool {
    version
        .split_once('+')
        .map_or(version, |(without_build, _)| without_build)
        .contains('-')
}

fn is_bounded_version(version: &str) -> bool {
    if version.is_empty()
        || version.len() > 64
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return false;
    }

    let without_build = match version.split_once('+') {
        Some((base, build)) if !build.contains('+') && valid_version_identifiers(build, false) => {
            base
        }
        Some(_) => return false,
        None => version,
    };
    let core = match without_build.split_once('-') {
        Some((core, prerelease)) if valid_version_identifiers(prerelease, true) => core,
        Some(_) => return false,
        None => without_build,
    };
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == &"0" || !part.starts_with('0'))
        })
}

fn valid_version_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!reject_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || identifier == "0"
                    || !identifier.starts_with('0'))
        })
}

struct PackagedArtifactSmoke {
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
    tools: PathBuf,
    binary: PathBuf,
    python: PathBuf,
    autosync_started: bool,
}

impl PackagedArtifactSmoke {
    fn new(
        repository_root: &Path,
        binary: &str,
        worker: &str,
        fixture: &str,
    ) -> Result<Self, String> {
        let binary = regular_input_file(repository_root, binary, "packaged binary")?;
        let worker = regular_input_file(repository_root, worker, "packaged Python worker")?;
        let fixture = regular_input_file(repository_root, fixture, "committed Pydantic fixture")?;
        let expected_worker = binary
            .parent()
            .ok_or_else(|| "packaged binary layout is invalid".to_string())?
            .join("workers/python/worker.py");
        let expected_worker = fs::canonicalize(expected_worker)
            .map_err(|_| "packaged worker layout is invalid".to_string())?;
        if worker != expected_worker {
            return Err("packaged worker layout is invalid".to_string());
        }
        let root = unique_smoke_root();
        let home = root.join("home");
        let project = root.join("project");
        let tools = root.join("tools");
        fs::create_dir_all(&home).map_err(|_| "could not create isolated HOME".to_string())?;
        fs::create_dir_all(&project)
            .map_err(|_| "could not create isolated fixture repository".to_string())?;
        fs::create_dir_all(&tools)
            .map_err(|_| "could not create isolated tool PATH".to_string())?;
        let python = prepare_isolated_tool_path(&tools)?;
        fs::copy(fixture, project.join("schemas.py"))
            .map_err(|_| "could not stage committed Pydantic fixture".to_string())?;
        Ok(Self {
            root,
            home,
            project,
            tools,
            binary,
            python,
            autosync_started: false,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .env_clear()
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_DATA_HOME", self.home.join(".local/share"))
            .env("XDG_CACHE_HOME", self.home.join(".cache"))
            .env("CODEX_HOME", self.home.join(".codex"))
            .env("PATH", &self.tools)
            .env("REPOGRAMMAR_PYTHON_EXECUTABLE", &self.python);
        command
    }

    fn run_text(&self, stage: &'static str, args: &[&str]) -> Result<String, String> {
        let output = self
            .command()
            .args(args)
            .output()
            .map_err(|_| format!("{stage} could not execute"))?;
        if !output.status.success() {
            return Err(format!("{stage} returned a failure status"));
        }
        String::from_utf8(output.stdout).map_err(|_| format!("{stage} output was not UTF-8"))
    }

    fn run_json(&self, stage: &'static str, args: &[&str]) -> Result<serde_json::Value, String> {
        let stdout = self.run_text(stage, args)?;
        serde_json::from_str(stdout.trim()).map_err(|_| format!("{stage} output was not JSON"))
    }

    fn project_text(&self) -> Result<String, String> {
        self.project
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| "isolated project path was not UTF-8".to_string())
    }

    fn append_third_pydantic_model(&self) -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(self.project.join("schemas.py"))
            .map_err(|_| "could not modify isolated Pydantic fixture".to_string())?;
        file.write_all(b"\n\nclass AuditRead(BaseModel):\n    id: int\n")
            .map_err(|_| "could not modify isolated Pydantic fixture".to_string())
    }
}

impl Drop for PackagedArtifactSmoke {
    fn drop(&mut self) {
        if self.autosync_started {
            let _ = self
                .command()
                .args([
                    "autosync",
                    "stop",
                    "--project",
                    self.project.to_string_lossy().as_ref(),
                    "--json",
                ])
                .output();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn smoke_packaged_artifact(
    repository_root: &Path,
    binary: &str,
    worker: &str,
    fixture: &str,
    expected_version: &str,
) -> Result<(), String> {
    if !is_bounded_version(expected_version) {
        return Err("expected version is invalid".to_string());
    }
    let mut smoke = PackagedArtifactSmoke::new(repository_root, binary, worker, fixture)?;
    let project = smoke.project_text()?;

    let version = smoke.run_text("version", &["version"])?;
    if version.trim() != format!("repogrammar {expected_version}") {
        return Err("version did not match release manifests".to_string());
    }

    let dry_run = smoke.run_json(
        "setup dry-run",
        &[
            "setup",
            "--project",
            &project,
            "--target",
            "auto",
            "--dry-run",
            "--no-autosync",
            "--json",
            "--progress",
            "never",
        ],
    )?;
    if dry_run["status"] != "dry_run" || dry_run["repository_index_ready"] != false {
        return Err("setup dry-run readiness was not truthful".to_string());
    }

    let setup = smoke.run_json(
        "live setup",
        &[
            "setup",
            "--project",
            &project,
            "--target",
            "auto",
            "--yes",
            "--no-autosync",
            "--json",
            "--progress",
            "never",
        ],
    )?;
    if setup["product_self_test_state"] != "passed"
        || setup["repository_index_ready"] != true
        || setup["agent_query_ready"] != false
        || !setup["suggested_question"].is_null()
    {
        return Err("live setup or product MCP self-test evidence was not truthful".to_string());
    }

    let resync = smoke.run_json(
        "Pydantic resync",
        &[
            "resync",
            "--project",
            &project,
            "--json",
            "--progress",
            "never",
        ],
    )?;
    let resync_generation = json_string(&resync, "generation_id", "Pydantic resync")?;
    if resync["status"] != "complete" || resync["command"] != "resync" {
        return Err("Pydantic resync did not complete".to_string());
    }

    let incremental = smoke.run_json(
        "Pydantic incremental sync",
        &[
            "sync",
            "--project",
            &project,
            "--json",
            "--progress",
            "never",
        ],
    )?;
    let incremental_generation =
        json_string(&incremental, "generation_id", "Pydantic incremental sync")?;
    if incremental["status"] != "complete"
        || incremental["command"] != "sync"
        || incremental["sync_mode"] != "incremental"
        || incremental["base_generation"].as_str() != Some(resync_generation.as_str())
        || incremental["reparsed_files"] != 0
        || incremental_generation == resync_generation
    {
        return Err("unchanged Pydantic incremental sync was not a copy-forward".to_string());
    }

    let started = smoke.run_json(
        "autosync start",
        &[
            "autosync",
            "start",
            "--project",
            &project,
            "--poll-ms",
            "100",
            "--debounce-ms",
            "50",
            "--json",
        ],
    )?;
    if started["running"] != true
        || started["startup_state"] != "ready"
        || started["daemon_state"] != "running"
    {
        return Err("autosync start did not prove ready daemon ownership".to_string());
    }
    smoke.autosync_started = true;

    std::thread::sleep(std::time::Duration::from_millis(400));
    let after_three_polls = smoke.run_json(
        "autosync three-poll status",
        &["autosync", "status", "--project", &project, "--json"],
    )?;
    if after_three_polls["running"] != true
        || after_three_polls["startup_state"] != "ready"
        || after_three_polls["daemon_state"] != "running"
        || after_three_polls["repository_ready"] != true
    {
        return Err(format!(
            "autosync did not remain ready for three poll intervals (running={}, startup_state={}, daemon_state={}, repository_ready={}, startup_failure_code={})",
            json_bool_label(&after_three_polls, "running"),
            json_string_label(&after_three_polls, "startup_state"),
            json_string_label(&after_three_polls, "daemon_state"),
            json_bool_label(&after_three_polls, "repository_ready"),
            json_string_label(&after_three_polls, "startup_failure_code"),
        ));
    }

    smoke.append_third_pydantic_model()?;
    let mut activated_generation = None;
    for _attempt in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let status = smoke.run_json(
            "autosync generation status",
            &["status", "--project", &project, "--json"],
        )?;
        if let Some(generation) = status["active_generation"].as_str() {
            if generation != incremental_generation {
                activated_generation = Some(generation.to_string());
                break;
            }
        }
        let daemon = smoke.run_json(
            "autosync liveness status",
            &["autosync", "status", "--project", &project, "--json"],
        )?;
        if daemon["running"] != true {
            return Err(format!(
                "autosync was not verifiably running before activating the changed generation (startup_state={}, daemon_state={}, repository_ready={}, startup_failure_code={})",
                json_string_label(&daemon, "startup_state"),
                json_string_label(&daemon, "daemon_state"),
                json_bool_label(&daemon, "repository_ready"),
                json_string_label(&daemon, "startup_failure_code"),
            ));
        }
    }
    if activated_generation.is_none() {
        return Err("autosync did not activate a changed generation before timeout".to_string());
    }
    let after_activation = smoke.run_json(
        "autosync post-activation status",
        &["autosync", "status", "--project", &project, "--json"],
    )?;
    if after_activation["running"] != true
        || after_activation["startup_state"] != "ready"
        || after_activation["daemon_state"] != "running"
    {
        return Err("autosync did not remain ready after generation activation".to_string());
    }

    let find = smoke.run_json(
        "packaged find",
        &["find", "--project", &project, "--json", "schemas.py"],
    )?;
    if find["status"] != "ok" || find["query_route"]["selected_family_id"].is_null() {
        return Err("packaged find did not select the Pydantic family".to_string());
    }
    let check = smoke.run_json(
        "packaged check",
        &["check", "--project", &project, "--json", "schemas.py"],
    )?;
    if check["status"] != "CONTEXT_ONLY" || check["check"]["advisory_status"] != "UNKNOWN" {
        return Err("packaged check did not preserve advisory UNKNOWN".to_string());
    }

    let stopped = smoke.run_json(
        "autosync stop",
        &["autosync", "stop", "--project", &project, "--json"],
    )?;
    if stopped["running"] != false || stopped["daemon_state"] != "stopped" {
        return Err("autosync stop did not report a stopped daemon".to_string());
    }
    smoke.autosync_started = false;
    let stopped_status = smoke.run_json(
        "autosync stopped status",
        &["autosync", "status", "--project", &project, "--json"],
    )?;
    if stopped_status["running"] != false
        || stopped_status["daemon_state"] != "stopped"
        || stopped_status["startup_state"] != "idle"
        || smoke
            .project
            .join(".repogrammar/locks/daemon.lock")
            .exists()
    {
        return Err("autosync stop left daemon readiness ownership behind".to_string());
    }
    Ok(())
}

fn json_string(
    value: &serde_json::Value,
    key: &str,
    stage: &'static str,
) -> Result<String, String> {
    value[key]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{stage} omitted {key}"))
}

fn json_bool_label(value: &serde_json::Value, key: &str) -> &'static str {
    match value[key].as_bool() {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

fn json_string_label<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("none")
}

fn regular_input_file(root: &Path, input: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(input);
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    let metadata = fs::symlink_metadata(&path).map_err(|_| format!("{label} is unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label} must be a regular file"));
    }
    fs::canonicalize(path).map_err(|_| format!("{label} is unavailable"))
}

fn unique_smoke_root() -> PathBuf {
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!(
        "repogrammar-packaged-smoke-{}-{started}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn prepare_isolated_tool_path(tools: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::symlink;
    let git = executable_on_path("git").ok_or_else(|| "git is unavailable".to_string())?;
    let python =
        executable_on_path("python3").ok_or_else(|| "Python 3 is unavailable".to_string())?;
    let kill = executable_on_path("kill").ok_or_else(|| "kill is unavailable".to_string())?;
    let ps = executable_on_path("ps").ok_or_else(|| "ps is unavailable".to_string())?;
    symlink(git, tools.join("git")).map_err(|_| "could not isolate git".to_string())?;
    symlink(&python, tools.join("python3"))
        .map_err(|_| "could not isolate Python 3".to_string())?;
    symlink(kill, tools.join("kill")).map_err(|_| "could not isolate kill".to_string())?;
    symlink(ps, tools.join("ps")).map_err(|_| "could not isolate ps".to_string())?;
    Ok(tools.join("python3"))
}

#[cfg(not(unix))]
fn prepare_isolated_tool_path(_tools: &Path) -> Result<PathBuf, String> {
    Err("packaged artifact smoke supports the declared macOS/Linux release hosts only".to_string())
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn run_check(root: &Path) -> CommandResult {
    match check_repository(root) {
        Ok(violations) if violations.is_empty() => CommandResult::ok("repository guard passed\n"),
        Ok(violations) => {
            let mut stderr = String::from("repository guard failed:\n");
            for violation in violations {
                stderr.push_str(&format!(
                    "- {}: {} ({})\n",
                    violation.path, violation.rule, violation.detail
                ));
            }
            CommandResult::err(stderr)
        }
        Err(error) => CommandResult::err(format!("repository guard failed: {error}\n")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GuardViolation {
    path: String,
    rule: &'static str,
    detail: String,
}

impl GuardViolation {
    fn new(path: impl Into<String>, rule: &'static str, detail: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            rule,
            detail: detail.into(),
        }
    }
}

fn check_repository(root: &Path) -> io::Result<Vec<GuardViolation>> {
    let mut violations = Vec::new();
    check_root_guides(root, &mut violations);
    check_required_documents(root, &mut violations);
    check_required_skills(root, &mut violations);
    check_github_workflow_actions(root, &mut violations)?;
    check_release_workflow_contract(root, &mut violations);
    check_tree_rules(root, &mut violations)?;
    Ok(violations)
}

fn check_root_guides(root: &Path, violations: &mut Vec<GuardViolation>) {
    for guide in ROOT_GUIDES.iter().copied() {
        let path = root.join(guide);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                violations.push(GuardViolation::new(
                    guide,
                    "RootGuideSymlink",
                    "root guide must be a regular file, not a symlink",
                ))
            }
            Ok(metadata) if !metadata.is_file() => violations.push(GuardViolation::new(
                guide,
                "RootGuideMissing",
                "root guide must be a regular file",
            )),
            Err(_) => violations.push(GuardViolation::new(
                guide,
                "RootGuideMissing",
                "root guide is required",
            )),
            Ok(_) => {}
        }
    }

    let agents = root.join("AGENTS.md");
    let claude = root.join("CLAUDE.md");
    if agents.is_file() && claude.is_file() {
        match (fs::read(&agents), fs::read(&claude)) {
            (Ok(left), Ok(right)) if left != right => violations.push(GuardViolation::new(
                "AGENTS.md|CLAUDE.md",
                "RootGuideDrift",
                "mirrored guides must be byte-for-byte identical",
            )),
            (Err(error), _) | (_, Err(error)) => violations.push(GuardViolation::new(
                "AGENTS.md|CLAUDE.md",
                "RootGuideRead",
                error.to_string(),
            )),
            _ => {}
        }
    }
}

fn check_required_documents(root: &Path, violations: &mut Vec<GuardViolation>) {
    for document in REQUIRED_DOCUMENTS.iter().copied() {
        if !root.join(document).is_file() {
            violations.push(GuardViolation::new(
                document,
                "RequiredDocumentMissing",
                "required bootstrap document or workflow is missing",
            ));
        }
    }
}

fn check_required_skills(root: &Path, violations: &mut Vec<GuardViolation>) {
    for skill in REQUIRED_SKILLS.iter().copied() {
        let skill_path = format!(".agents/skills/{skill}/SKILL.md");
        let path = root.join(&skill_path);
        match fs::read_to_string(&path) {
            Ok(content) => {
                if let Err(detail) = validate_skill_front_matter(&content, skill) {
                    violations.push(GuardViolation::new(
                        skill_path,
                        "SkillFrontMatterInvalid",
                        detail,
                    ));
                }
            }
            Err(_) => violations.push(GuardViolation::new(
                skill_path,
                "SkillMissing",
                "required skill file is missing",
            )),
        }
    }
}

fn validate_skill_front_matter(content: &str, expected_name: &str) -> Result<(), String> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Err("front matter must start with ---".to_string());
    }

    let mut name = None;
    let mut description = None;
    for line in lines.by_ref() {
        if line == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(value.trim());
        }
        if let Some(value) = line.strip_prefix("description:") {
            description = Some(value.trim());
        }
    }

    match name {
        Some(value) if value == expected_name => {}
        Some(value) => {
            return Err(format!(
                "front matter name must be {expected_name}, found {value}"
            ));
        }
        None => return Err("front matter must include name".to_string()),
    }

    match description {
        Some(value) if !value.is_empty() => Ok(()),
        _ => Err("front matter must include non-empty description".to_string()),
    }
}

fn check_github_workflow_actions(
    root: &Path,
    violations: &mut Vec<GuardViolation>,
) -> io::Result<()> {
    let workflows = root.join(".github").join("workflows");
    let entries = match fs::read_dir(&workflows) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in entries {
        let path = entry?.path();
        let Some(extension) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };
        if !matches!(extension, "yml" | "yaml") {
            continue;
        }
        let contents = fs::read_to_string(&path)?;
        let relative = workflow_relative_path(root, &path);
        for (deprecated, replacement) in DEPRECATED_NODE20_ACTIONS {
            if contents.contains(deprecated) {
                violations.push(GuardViolation::new(
                    relative.clone(),
                    "DeprecatedGitHubActionRuntime",
                    format!(
                        "{deprecated} targets the deprecated Node.js 20 runtime; {replacement}"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn check_release_workflow_contract(root: &Path, violations: &mut Vec<GuardViolation>) {
    let release_path = root.join(".github/workflows/release.yml");
    let reconcile_path = root.join(".github/workflows/npm-tag-reconcile.yml");
    let (Ok(release), Ok(reconcile)) = (
        fs::read_to_string(&release_path),
        fs::read_to_string(&reconcile_path),
    ) else {
        return;
    };

    let release_publish = release.find("npm publish --access public --tag preview");
    let release_reconcile = release.find("uses: ./.github/workflows/npm-tag-reconcile.yml");
    if !matches!((release_publish, release_reconcile), (Some(publish), Some(reconcile)) if publish < reconcile)
    {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "ReleaseDistTagOrder",
            "preview publication must call npm tag reconciliation after npm publish",
        ));
    }

    let reconciliation_markers = [
        "workflow_dispatch:",
        "preview-dist-tag-action",
        "npm dist-tag rm",
        "tags_after=",
        "final_action=",
    ];
    if reconcile.contains("npm publish")
        || reconciliation_markers
            .iter()
            .any(|marker| !reconcile.contains(marker))
    {
        violations.push(GuardViolation::new(
            ".github/workflows/npm-tag-reconcile.yml",
            "NpmTagReconciliationContract",
            "manual repair must be non-publishing and verify registry state after bounded reconciliation",
        ));
    }
}

fn workflow_relative_path(root: &Path, path: &Path) -> String {
    relative_path(root, path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn check_tree_rules(root: &Path, violations: &mut Vec<GuardViolation>) -> io::Result<()> {
    walk_files(root, root, &mut |path| {
        let relative = relative_path(root, path);
        if is_lowercase_duplicate(&relative) {
            violations.push(GuardViolation::new(
                relative.display().to_string(),
                "LowercaseGuideDuplicate",
                "lowercase agents.md or claude.md is not allowed",
            ));
        }
        if is_nested_root_guide(&relative) {
            violations.push(GuardViolation::new(
                relative.display().to_string(),
                "NestedGuide",
                "nested AGENTS.md or CLAUDE.md is not allowed",
            ));
        }
        if is_source_file(path) && !is_under_src(&relative) {
            violations.push(GuardViolation::new(
                relative.display().to_string(),
                "SourceOutsideSrc",
                "source, script, SQL, test, fixture, or automation code must live under src/",
            ));
        }
    })
}

fn walk_files<F>(root: &Path, current: &Path, visit: &mut F) -> io::Result<()>
where
    F: FnMut(&Path),
{
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            if should_skip_dir(root, &path) {
                continue;
            }
            walk_files(root, &path, visit)?;
        } else if metadata.is_file() || metadata.file_type().is_symlink() {
            visit(&path);
        }
    }
    Ok(())
}

fn should_skip_dir(root: &Path, path: &Path) -> bool {
    if path == root {
        return false;
    }
    if is_linked_agent_worktree(root, path) {
        return true;
    }
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            IGNORED_DIRS.contains(&name)
                || IGNORED_DIR_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
        })
}

fn is_linked_agent_worktree(root: &Path, path: &Path) -> bool {
    let agent_worktrees = root.join(".claude").join("worktrees");
    if path.parent() != Some(agent_worktrees.as_path()) {
        return false;
    }

    let git_pointer = path.join(".git");
    let Ok(metadata) = fs::symlink_metadata(&git_pointer) else {
        return false;
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_GITDIR_POINTER_BYTES
    {
        return false;
    }

    let Ok(file) = fs::File::open(&git_pointer) else {
        return false;
    };
    let mut contents = String::new();
    let Ok(bytes_read) = file
        .take(MAX_GITDIR_POINTER_BYTES + 1)
        .read_to_string(&mut contents)
    else {
        return false;
    };
    if bytes_read as u64 > MAX_GITDIR_POINTER_BYTES {
        return false;
    }
    let Some(raw_git_dir) = contents.trim().strip_prefix("gitdir: ") else {
        return false;
    };
    let Ok(authorized_root) = fs::canonicalize(root.join(".git").join("worktrees")) else {
        return false;
    };
    let Ok(resolved_git_dir) = fs::canonicalize(raw_git_dir) else {
        return false;
    };
    resolved_git_dir.parent() == Some(authorized_root.as_path())
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn is_lowercase_duplicate(relative: &Path) -> bool {
    relative
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| matches!(name, "agents.md" | "claude.md"))
}

fn is_nested_root_guide(relative: &Path) -> bool {
    let name = relative.file_name().and_then(OsStr::to_str);
    matches!(name, Some("AGENTS.md" | "CLAUDE.md"))
        && relative != Path::new("AGENTS.md")
        && relative != Path::new("CLAUDE.md")
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension.as_str()))
}

fn is_under_src(relative: &Path) -> bool {
    relative
        .components()
        .next()
        .is_some_and(|component| component.as_os_str() == OsStr::new("src"))
}

fn sync_agent_guides(root: &Path, source: &str) -> Result<(), String> {
    let target = match source {
        "AGENTS.md" => "CLAUDE.md",
        "CLAUDE.md" => "AGENTS.md",
        _ => return Err("--from must be exactly AGENTS.md or CLAUDE.md".to_string()),
    };

    let source_path = root.join(source);
    let target_path = root.join(target);
    ensure_regular_non_symlink(&source_path, source)?;
    if target_path.exists() {
        ensure_regular_non_symlink(&target_path, target)?;
    }

    let bytes =
        fs::read(&source_path).map_err(|error| format!("failed to read {source}: {error}"))?;
    fs::write(&target_path, bytes).map_err(|error| format!("failed to write {target}: {error}"))?;

    let left = fs::read(root.join("AGENTS.md"))
        .map_err(|error| format!("failed to read AGENTS.md after sync: {error}"))?;
    let right = fs::read(root.join("CLAUDE.md"))
        .map_err(|error| format!("failed to read CLAUDE.md after sync: {error}"))?;
    if left == right {
        Ok(())
    } else {
        Err("agent guide sync completed but byte comparison still differs".to_string())
    }
}

fn ensure_regular_non_symlink(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("{label} must not be a symlink"))
        }
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(format!("{label} must be a regular file")),
        Err(error) => Err(format!("{label} is required: {error}")),
    }
}

fn changed_paths_between(root: &Path, base: &str, head: &str) -> Result<Vec<String>, String> {
    if base.trim().is_empty() || head.trim().is_empty() {
        return Err("check-diff requires non-empty base and head revisions".to_string());
    }

    let output = Command::new("git")
        .args(["diff", "--name-only", base, head])
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to execute git diff: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "failed to compare revisions {base}..{head}; initial commits may not have a base revision: {}",
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(ToOwned::to_owned).collect())
}

fn check_diff_paths(paths: &[String]) -> Result<(), String> {
    let has_src_change = paths
        .iter()
        .any(|path| path == "src" || path.starts_with("src/"));
    let has_doc_change = paths.iter().any(|path| {
        path.starts_with("docs/")
            || path.starts_with(".agents/skills/")
            || path.starts_with(".agents/memories/")
            || matches!(
                path.as_str(),
                "README.md" | "CHANGELOG.md" | "AGENTS.md" | "CLAUDE.md"
            )
    });

    if has_src_change && !has_doc_change {
        Err(
            "diff changes src/ without any required documentation or agent-material change"
                .to_string(),
        )
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preview_dist_tags_without_latest_need_no_write() {
        assert_eq!(
            preview_dist_tag_action("0.2.0-preview.0", "0.2.0-preview.0", ""),
            Ok(PreviewDistTagAction::NoTag)
        );
    }

    #[test]
    fn prerelease_latest_is_removed_even_when_it_is_an_older_preview() {
        for latest in ["0.2.0-preview.0", "0.1.0-preview.9"] {
            assert_eq!(
                preview_dist_tag_action("0.2.0-preview.0", "0.2.0-preview.0", latest),
                Ok(PreviewDistTagAction::RemovePrerelease)
            );
        }
    }

    #[test]
    fn stable_latest_is_preserved() {
        for latest in ["0.1.0", "0.1.0+build-x"] {
            assert_eq!(
                preview_dist_tag_action("0.2.0-preview.0", "0.2.0-preview.0", latest),
                Ok(PreviewDistTagAction::PreserveStable)
            );
        }
    }

    #[test]
    fn preview_dist_tag_classification_fails_closed() {
        assert_eq!(
            preview_dist_tag_action("0.2.0-preview.0", "", ""),
            Err("preview does not match the manifest version")
        );
        assert_eq!(
            preview_dist_tag_action("0.2.0-preview.0", "0.2.0-preview.0", "banana"),
            Err("latest is malformed")
        );
        assert_eq!(
            preview_dist_tag_action("0.2.0", "0.2.0", ""),
            Err("manifest version is not a bounded prerelease")
        );
    }

    #[test]
    fn release_workflow_requires_post_publish_reconciliation() {
        let root = TempRoot::new("release-workflow-order");
        write_file(
            root.path().join(".github/workflows/release.yml"),
            b"run: npm publish --access public --tag preview\nuses: ./.github/workflows/npm-tag-reconcile.yml\n",
        );
        write_file(
            root.path().join(".github/workflows/npm-tag-reconcile.yml"),
            b"workflow_dispatch:\npreview-dist-tag-action\nnpm dist-tag rm\ntags_after=\nfinal_action=\n",
        );

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations.is_empty());

        write_file(
            root.path().join(".github/workflows/release.yml"),
            b"uses: ./.github/workflows/npm-tag-reconcile.yml\nrun: npm publish --access public --tag preview\n",
        );
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations
            .iter()
            .any(|violation| violation.rule == "ReleaseDistTagOrder"));
    }

    #[test]
    fn manual_tag_repair_workflow_cannot_publish() {
        let root = TempRoot::new("manual-tag-repair-publish");
        write_file(
            root.path().join(".github/workflows/release.yml"),
            b"run: npm publish --access public --tag preview\nuses: ./.github/workflows/npm-tag-reconcile.yml\n",
        );
        write_file(
            root.path().join(".github/workflows/npm-tag-reconcile.yml"),
            b"workflow_dispatch:\npreview-dist-tag-action\nnpm dist-tag rm\ntags_after=\nfinal_action=\nnpm publish\n",
        );

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations
            .iter()
            .any(|violation| violation.rule == "NpmTagReconciliationContract"));
    }

    #[test]
    fn packaged_artifact_smoke_is_a_documented_command() {
        assert!(usage().contains(
            "smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version>"
        ));
    }

    #[test]
    fn packaged_artifact_smoke_rejects_missing_inputs_without_repository_state() {
        let root = TempRoot::new("packaged-smoke-missing-input");

        let error = smoke_packaged_artifact(
            root.path(),
            "missing-repogrammar",
            "missing-worker.py",
            "missing-fixture.py",
            "0.2.0-preview.0",
        )
        .expect_err("missing packaged artifact must fail closed");

        assert_eq!(error, "packaged binary is unavailable");
        assert!(!root.path().join(".repogrammar").exists());
    }

    #[test]
    fn packaged_artifact_smoke_requires_the_bundled_worker_layout() {
        let root = TempRoot::new("packaged-smoke-worker-layout");
        write_file(root.path().join("package/repogrammar"), b"not executed\n");
        write_file(root.path().join("other/worker.py"), b"# not bundled\n");
        write_file(
            root.path().join("fixture.py"),
            b"class Example:\n    pass\n",
        );

        let error = smoke_packaged_artifact(
            root.path(),
            "package/repogrammar",
            "other/worker.py",
            "fixture.py",
            "0.2.0-preview.0",
        )
        .expect_err("worker outside packaged layout must fail closed");

        assert_eq!(error, "packaged worker layout is invalid");
    }

    #[cfg(unix)]
    #[test]
    fn packaged_artifact_smoke_rejects_symlinked_inputs() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("packaged-smoke-symlink");
        write_file(root.path().join("repogrammar-real"), b"not executed\n");
        symlink(
            root.path().join("repogrammar-real"),
            root.path().join("repogrammar"),
        )
        .expect("create packaged binary symlink");

        let error = smoke_packaged_artifact(
            root.path(),
            "repogrammar",
            "missing-worker.py",
            "missing-fixture.py",
            "0.2.0-preview.0",
        )
        .expect_err("symlinked packaged binary must fail closed");

        assert_eq!(error, "packaged binary must be a regular file");
    }

    #[test]
    fn skill_front_matter_requires_expected_name_and_description() {
        let content =
            "---\nname: rust-quality\ndescription: Run Rust quality gates.\n---\n# Body\n";

        assert!(validate_skill_front_matter(content, "rust-quality").is_ok());
        assert!(validate_skill_front_matter(content, "other").is_err());
    }

    #[test]
    fn sync_agent_guides_copies_source_bytes() {
        let root = TempRoot::new("sync");
        write_file(root.path().join("AGENTS.md"), b"# A\n");
        write_file(root.path().join("CLAUDE.md"), b"# B\n");

        sync_agent_guides(root.path(), "AGENTS.md").expect("sync succeeds");

        assert_eq!(
            fs::read(root.path().join("AGENTS.md")).expect("read AGENTS"),
            fs::read(root.path().join("CLAUDE.md")).expect("read CLAUDE")
        );
    }

    #[test]
    fn check_reports_guide_drift() {
        let root = TempRoot::new("drift");
        write_file(root.path().join("AGENTS.md"), b"# A\n");
        write_file(root.path().join("CLAUDE.md"), b"# B\n");

        let violations = check_repository(root.path()).expect("check repository");

        assert!(violations
            .iter()
            .any(|violation| violation.rule == "RootGuideDrift"));
    }

    #[test]
    fn required_documents_include_repo_local_state_boundary_adr() {
        let root = TempRoot::new("missing-adr-0008");
        for document in REQUIRED_DOCUMENTS
            .iter()
            .copied()
            .filter(|document| *document != "docs/decisions/ADR-0008-repo-local-state-boundary.md")
        {
            write_file(root.path().join(document), b"present\n");
        }

        let mut violations = Vec::new();
        check_required_documents(root.path(), &mut violations);

        assert!(violations.iter().any(|violation| {
            violation.path == "docs/decisions/ADR-0008-repo-local-state-boundary.md"
                && violation.rule == "RequiredDocumentMissing"
        }));
    }

    #[test]
    fn required_documents_include_v0_1_planning_artifacts() {
        let root = TempRoot::new("missing-v0-1-planning-docs");
        let missing_documents = [
            "docs/plans/v0.1-parallel-development-plan.md",
            "docs/plans/python-dogfooding-plan.md",
            "docs/plans/python-v0.1-implementation-plan.md",
            "docs/plans/codegraph-provider-plan.md",
            "docs/plans/v0.1-substrate-hardening-checkpoint.md",
            "docs/specifications/unknowns.md",
            "docs/specifications/python-analysis.md",
            "docs/decisions/ADR-0009-experimental-python-dogfooding.md",
            "docs/decisions/ADR-0010-optional-codegraph-provider.md",
            "docs/decisions/ADR-0011-python-first-v0-1.md",
            "docs/decisions/ADR-0012-python-selective-analysis-cascade.md",
            ".agents/memories/v0.1-parallel-development-plan.md",
            ".agents/memories/python-dogfooding-plan.md",
            ".agents/memories/python-v0.1-algorithm-plan.md",
            ".agents/memories/codegraph-provider-plan.md",
            ".agents/memories/unknown-governance.md",
            ".agents/memories/v0.1-substrate-hardening-checkpoint.md",
        ];
        for document in REQUIRED_DOCUMENTS
            .iter()
            .copied()
            .filter(|document| !missing_documents.contains(document))
        {
            write_file(root.path().join(document), b"present\n");
        }

        let mut violations = Vec::new();
        check_required_documents(root.path(), &mut violations);

        for document in missing_documents {
            assert!(
                violations.iter().any(|violation| {
                    violation.path == document && violation.rule == "RequiredDocumentMissing"
                }),
                "missing required-document violation for {document}"
            );
        }
    }

    #[test]
    fn required_documents_include_top_20_language_expansion_authority() {
        let root = TempRoot::new("missing-top-20-language-expansion-docs");
        let missing_documents = [
            "docs/decisions/ADR-0020-top-20-language-expansion-gate.md",
            "docs/plans/top-20-language-expansion-plan.md",
        ];
        for document in REQUIRED_DOCUMENTS
            .iter()
            .copied()
            .filter(|document| !missing_documents.contains(document))
        {
            write_file(root.path().join(document), b"present\n");
        }

        let mut violations = Vec::new();
        check_required_documents(root.path(), &mut violations);

        for document in missing_documents {
            assert!(
                violations.iter().any(|violation| {
                    violation.path == document && violation.rule == "RequiredDocumentMissing"
                }),
                "missing required-document violation for {document}"
            );
        }
    }

    #[test]
    fn source_outside_src_is_reported() {
        let root = TempRoot::new("outside-src");
        write_file(root.path().join("AGENTS.md"), b"# Same\n");
        write_file(root.path().join("CLAUDE.md"), b"# Same\n");
        write_file(root.path().join("tool.py"), b"print('no')\n");

        let violations = check_repository(root.path()).expect("check repository");

        assert!(violations.iter().any(|violation| {
            violation.rule == "SourceOutsideSrc" && violation.path == "tool.py"
        }));
    }

    #[test]
    fn csharp_source_outside_src_is_reported() {
        let root = TempRoot::new("outside-src-csharp");
        write_file(root.path().join("AGENTS.md"), b"# Same\n");
        write_file(root.path().join("CLAUDE.md"), b"# Same\n");
        write_file(
            root.path().join("Program.cs"),
            b"public class Program { }\n",
        );

        let violations = check_repository(root.path()).expect("check repository");

        assert!(violations.iter().any(|violation| {
            violation.rule == "SourceOutsideSrc" && violation.path == "Program.cs"
        }));
    }

    #[test]
    fn cpp_extension_sources_outside_src_are_reported() {
        let root = TempRoot::new("outside-src-cpp");
        write_file(root.path().join("AGENTS.md"), b"# Same\n");
        write_file(root.path().join("CLAUDE.md"), b"# Same\n");
        write_file(
            root.path().join("widget.cxx"),
            b"int main() { return 0; }\n",
        );
        write_file(root.path().join("widget.hh"), b"struct Widget {};\n");
        write_file(root.path().join("legacy.hxx"), b"struct Legacy {};\n");

        let violations = check_repository(root.path()).expect("check repository");

        for path in ["widget.cxx", "widget.hh", "legacy.hxx"] {
            assert!(
                violations.iter().any(|violation| {
                    violation.rule == "SourceOutsideSrc" && violation.path == path
                }),
                "expected SourceOutsideSrc for {path}"
            );
        }
    }

    #[test]
    fn repo_local_state_directories_are_ignored() {
        let root = TempRoot::new("state-dirs");
        write_file(root.path().join("AGENTS.md"), b"# Same\n");
        write_file(root.path().join("CLAUDE.md"), b"# Same\n");
        write_file(
            root.path().join(".repogrammar/cache/generated.py"),
            b"print('cache')\n",
        );
        write_file(
            root.path().join(".repogrammar-win/cache/generated.ts"),
            b"export const cache = true;\n",
        );

        let violations = check_repository(root.path()).expect("check repository");

        assert!(!violations.iter().any(|violation| {
            violation.rule == "SourceOutsideSrc" && violation.path.starts_with(".repogrammar")
        }));
    }

    #[test]
    fn agent_worktree_state_is_ignored_without_ignoring_other_worktrees() {
        let root = TempRoot::new("agent-worktrees");
        write_file(root.path().join("AGENTS.md"), b"# Same\n");
        write_file(root.path().join("CLAUDE.md"), b"# Same\n");
        let linked_git_dir = root.path().join(".git/worktrees/agent");
        fs::create_dir_all(&linked_git_dir).expect("create linked-worktree git dir");
        let linked_git_dir = fs::canonicalize(linked_git_dir).expect("canonical git dir");
        let git_pointer = format!("gitdir: {}\n", linked_git_dir.display());
        write_file(
            root.path().join(".claude/worktrees/agent/.git"),
            git_pointer.as_bytes(),
        );
        write_file(
            root.path().join(".claude/worktrees/agent/AGENTS.md"),
            b"# Isolated checkout\n",
        );
        write_file(
            root.path().join(".claude/worktrees/agent/tool.py"),
            b"print('isolated checkout')\n",
        );
        write_file(
            root.path().join(".claude/worktrees/not-a-worktree/tool.py"),
            b"print('not a linked checkout')\n",
        );
        let oversized_pointer = vec![b'x'; (MAX_GITDIR_POINTER_BYTES + 1) as usize];
        write_file(
            root.path().join(".claude/worktrees/oversized/.git"),
            &oversized_pointer,
        );
        write_file(
            root.path().join(".claude/worktrees/oversized/tool.py"),
            b"print('oversized marker is not trusted')\n",
        );
        write_file(
            root.path().join("sandbox/worktrees/tool.py"),
            b"print('ordinary repository path')\n",
        );

        let violations = check_repository(root.path()).expect("check repository");

        assert!(!violations
            .iter()
            .any(|violation| violation.path.starts_with(".claude/worktrees/agent/")));
        assert!(violations.iter().any(|violation| {
            violation.rule == "SourceOutsideSrc"
                && violation.path == ".claude/worktrees/not-a-worktree/tool.py"
        }));
        assert!(violations.iter().any(|violation| {
            violation.rule == "SourceOutsideSrc"
                && violation.path == ".claude/worktrees/oversized/tool.py"
        }));
        assert!(violations.iter().any(|violation| {
            violation.rule == "SourceOutsideSrc" && violation.path == "sandbox/worktrees/tool.py"
        }));
    }

    #[test]
    fn deprecated_node20_github_actions_are_reported() {
        let root = TempRoot::new("node20-actions");
        write_file(
            root.path().join(".github/workflows/ci.yml"),
            b"steps:\n  - uses: actions/checkout@v4\n",
        );
        write_file(
            root.path().join(".github/workflows/release.yml"),
            b"steps:\n  - uses: actions/setup-node@v4\n",
        );

        let mut violations = Vec::new();
        check_github_workflow_actions(root.path(), &mut violations).expect("workflow check");

        assert!(violations.iter().any(|violation| {
            violation.path == ".github/workflows/ci.yml"
                && violation.rule == "DeprecatedGitHubActionRuntime"
                && violation.detail.contains("actions/checkout@v5")
        }));
        assert!(violations.iter().any(|violation| {
            violation.path == ".github/workflows/release.yml"
                && violation.rule == "DeprecatedGitHubActionRuntime"
                && violation.detail.contains("actions/setup-node@v5")
        }));
    }

    #[test]
    fn diff_gate_requires_docs_when_src_changes() {
        assert!(check_diff_paths(&["src/rust/lib.rs".to_string()]).is_err());
        assert!(check_diff_paths(&[
            "src/rust/lib.rs".to_string(),
            "docs/architecture/module-map.md".to_string()
        ])
        .is_ok());
        assert!(check_diff_paths(&["README.md".to_string()]).is_ok());
    }

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(prefix: &str) -> Self {
            let mut path = env::temp_dir();
            path.push(format!(
                "repo-guard-{prefix}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time after epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("create temp root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: PathBuf, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write file");
    }
}
