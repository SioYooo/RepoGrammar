use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use repogrammar::application::install::{managed_instruction_block, MANAGED_INSTRUCTION_VERSION};
use sha2::{Digest, Sha256, Sha512};

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
    ".github/workflows/stable-release-finalize.yml",
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
const STABLE_RELEASE_VERSION: &str = "0.2.2";
const STABLE_PREVIEW_VERSION: &str = "0.2.0-preview.0";
const FAILED_STABLE_RELEASE_VERSIONS: &[&str] = &["0.2.0", "0.2.1"];
const PREVIEW_NPM_STAGE_PACKAGE_ASSIGNMENT: &str = r#"package_file="./npm-candidate/sioyooo-repogrammar-${{ needs.classify.outputs.version }}.tgz""#;
const STABLE_NPM_STAGE_COMMAND: &str =
    "npm stage publish ./npm-candidate/sioyooo-repogrammar-0.2.2.tgz --access public --tag latest --provenance";
const NPM_PACKAGE_NAME: &str = "@sioyooo/repogrammar";
const MAX_NPM_CANDIDATE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RELEASE_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_GITHUB_OUTPUT_BYTES: u64 = 1024 * 1024;
const MAX_DSSE_PAYLOAD_BASE64_BYTES: usize = 1024 * 1024;
const NPM_REGISTRY_URL: &str = "https://registry.npmjs.org/";
const SLSA_PROVENANCE_V1: &str = "https://slsa.dev/provenance/v1";
const NPM_PUBLISH_ATTESTATION_V1: &str =
    "https://github.com/npm/attestation/tree/main/specs/publish/v0.1";
const IN_TOTO_STATEMENT_V1: &str = "https://in-toto.io/Statement/v1";
const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";
const GITHUB_WORKFLOW_BUILD_TYPE_V1: &str =
    "https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1";
const GITHUB_HOSTED_BUILDER: &str = "https://github.com/actions/runner/github-hosted";
const RELEASE_REPOSITORY_URL: &str = "https://github.com/SioYooo/RepoGrammar";
const RELEASE_WORKFLOW_PATH: &str = ".github/workflows/release.yml";
const RELEASE_DEPENDENCY_URI: &str = "git+https://github.com/SioYooo/RepoGrammar@refs/tags/v0.2.2";
const NPM_PACKAGE_FILES: &[&str] = &[
    "package/LICENSE",
    "package/README.md",
    "package/package.json",
    "package/src/npm/repogrammar.js",
];
const STABLE_RELEASE_ASSETS: &[&str] = &[
    "install.sh",
    "install.sh.sha256",
    "npm-candidate-manifest.json",
    "repogrammar-aarch64-apple-darwin.tar.gz",
    "repogrammar-aarch64-apple-darwin.tar.gz.sha256",
    "repogrammar-aarch64-unknown-linux-gnu.tar.gz",
    "repogrammar-aarch64-unknown-linux-gnu.tar.gz.sha256",
    "repogrammar-x86_64-apple-darwin.tar.gz",
    "repogrammar-x86_64-apple-darwin.tar.gz.sha256",
    "repogrammar-x86_64-unknown-linux-gnu.tar.gz",
    "repogrammar-x86_64-unknown-linux-gnu.tar.gz.sha256",
];

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
        [command, tarball_flag, tarball, version_flag, expected_version]
            if command == "smoke-npm-package"
                && tarball_flag == "--tarball"
                && version_flag == "--expected-version" =>
        {
            match smoke_npm_package(root, tarball, expected_version) {
                Ok(manifest) => CommandResult::ok(format!("{manifest}\n")),
                Err(error) => CommandResult::err(format!("npm package smoke failed: {error}\n")),
            }
        }
        [command, pack_flag, pack_json, manifest_flag, candidate_manifest, version_flag, expected_version]
            if command == "verify-npm-pack-evidence"
                && pack_flag == "--pack-json"
                && manifest_flag == "--candidate-manifest"
                && version_flag == "--expected-version" =>
        {
            match verify_npm_pack_evidence(root, pack_json, candidate_manifest, expected_version) {
                Ok(()) => CommandResult::ok("npm pack evidence passed\n"),
                Err(error) => CommandResult::err(format!("npm pack evidence failed: {error}\n")),
            }
        }
        [command, evidence_flag, evidence_dir]
            if command == "verify-stable-release-evidence" && evidence_flag == "--evidence-dir" =>
        {
            match verify_stable_release_evidence(root, evidence_dir) {
                Ok(()) => CommandResult::ok("STABLE_RELEASE_READY\n"),
                Err(error) => {
                    CommandResult::err(format!("stable release evidence failed: {error}\n"))
                }
            }
        }
        [command, version_flag, version, preview_flag, preview, latest_flag, latest, versions_flag, versions_json]
            if command == "preview-dist-tag-action"
                && version_flag == "--version"
                && preview_flag == "--preview"
                && latest_flag == "--latest"
                && versions_flag == "--versions-json" =>
        {
            match preview_dist_tag_action(version, preview, latest, versions_json) {
                Ok(action) => CommandResult::ok(format!("{}\n", action.as_str())),
                Err(error) => {
                    CommandResult::err(format!("preview dist-tag classification failed: {error}\n"))
                }
            }
        }
        [command, version_flag, version]
            if command == "release-channel" && version_flag == "--version" =>
        {
            match release_channel(version) {
                Ok(channel) => CommandResult::ok(format!("{}\n", channel.as_str())),
                Err(error) => {
                    CommandResult::err(format!("release channel classification failed: {error}\n"))
                }
            }
        }
        [command, event_flag, event_name, ref_flag, ref_name]
            if command == "release-source"
                && event_flag == "--event-name"
                && ref_flag == "--ref-name" =>
        {
            release_source_command(root, event_name, ref_name, env::var_os("GITHUB_OUTPUT"))
        }
        [command, version_flag, version, preview_flag, preview, latest_flag, latest, tags_flag, tags_json, versions_flag, versions_json]
            if command == "release-dist-tag-action"
                && version_flag == "--version"
                && preview_flag == "--preview"
                && latest_flag == "--latest"
                && tags_flag == "--tags-json"
                && versions_flag == "--versions-json" =>
        {
            match release_dist_tag_action(version, preview, latest, tags_json, versions_json) {
                Ok(action) => CommandResult::ok(format!("{}\n", action.as_str())),
                Err(error) => {
                    CommandResult::err(format!("release dist-tag classification failed: {error}\n"))
                }
            }
        }
        [] => CommandResult::err(format!("{}\n", usage())),
        _ => CommandResult::err(format!("unknown or invalid arguments\n{}\n", usage())),
    }
}

fn usage() -> &'static str {
    "Usage: repo-guard check | sync-agent-guides --from <AGENTS.md|CLAUDE.md> | check-diff --base <rev> --head <rev> | smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version> | smoke-npm-package --tarball <path> --expected-version <version> | verify-npm-pack-evidence --pack-json <path> --candidate-manifest <path> --expected-version <version> | verify-stable-release-evidence --evidence-dir <path> | release-source --event-name <workflow_dispatch|push> --ref-name <name> | release-channel --version <version> | release-dist-tag-action --version <version> --preview <version-or-empty> --latest <version-or-empty> --tags-json <json-object> --versions-json <json-array> | preview-dist-tag-action --version <version> --preview <version> --latest <version-or-empty> --versions-json <json-array>"
}

const MAX_PUBLISHED_VERSIONS_JSON_BYTES: usize = 16 * 1024;
const MAX_PUBLISHED_VERSIONS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseChannel {
    Preview,
    Stable,
}

impl ReleaseChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Stable => "stable",
        }
    }
}

fn release_channel(version: &str) -> Result<ReleaseChannel, &'static str> {
    if !is_bounded_version(version) {
        return Err("manifest version is malformed");
    }
    if has_prerelease(version) {
        Ok(ReleaseChannel::Preview)
    } else {
        Ok(ReleaseChannel::Stable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseSource {
    channel: ReleaseChannel,
    version: String,
}

fn release_source_lines(source: &ReleaseSource) -> String {
    format!(
        "channel={}\nversion={}\n",
        source.channel.as_str(),
        source.version
    )
}

fn append_release_source_github_output(
    source: &ReleaseSource,
    output_path: Option<std::ffi::OsString>,
) -> Result<(), String> {
    let Some(output_path) = output_path else {
        return Ok(());
    };
    if output_path.is_empty() {
        return Err("GitHub output path is malformed".to_string());
    }
    let output_path = PathBuf::from(output_path);
    let metadata = fs::symlink_metadata(&output_path)
        .map_err(|_| "GitHub output file is unavailable".to_string())?;
    let output = release_source_lines(source);
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("GitHub output file must be a bounded regular file".to_string());
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(output_path)
        .map_err(|_| "GitHub output file is unavailable".to_string())?;
    let opened_metadata = file
        .metadata()
        .map_err(|_| "GitHub output file is unavailable".to_string())?;
    #[cfg(unix)]
    let same_file = {
        use std::os::unix::fs::MetadataExt;
        metadata.dev() == opened_metadata.dev() && metadata.ino() == opened_metadata.ino()
    };
    #[cfg(not(unix))]
    let same_file = metadata.len() == opened_metadata.len();
    if !same_file
        || !opened_metadata.is_file()
        || opened_metadata
            .len()
            .checked_add(output.len() as u64)
            .is_none_or(|size| size > MAX_GITHUB_OUTPUT_BYTES)
    {
        return Err("GitHub output file must be a bounded regular file".to_string());
    }
    file.write_all(output.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|_| "GitHub output file could not be written".to_string())
}

fn release_source_command(
    root: &Path,
    event_name: &str,
    ref_name: &str,
    output_path: Option<std::ffi::OsString>,
) -> CommandResult {
    match release_source(root, event_name, ref_name) {
        Ok(source) => match append_release_source_github_output(&source, output_path) {
            Ok(()) => CommandResult::ok(release_source_lines(&source)),
            Err(error) => CommandResult::err(format!("release source failed: {error}\n")),
        },
        Err(error) => CommandResult::err(format!("release source failed: {error}\n")),
    }
}

fn release_source(root: &Path, event_name: &str, ref_name: &str) -> Result<ReleaseSource, String> {
    if !matches!(event_name, "workflow_dispatch" | "push") {
        return Err("release event is unsupported".to_string());
    }
    if ref_name.is_empty()
        || ref_name.len() > 256
        || !ref_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-'))
    {
        return Err("release ref name is malformed".to_string());
    }

    let package_version = package_json_release_version(root)?;
    let cargo_version = cargo_manifest_release_version(root)?;
    let lock_version = cargo_lock_release_version(root)?;
    if package_version != cargo_version || package_version != lock_version {
        return Err("release manifest versions do not match".to_string());
    }

    let channel = release_channel(&package_version).map_err(str::to_string)?;
    if event_name == "push" {
        let expected_ref = format!("v{package_version}");
        if ref_name != expected_ref {
            return Err("release tag does not match the manifest version".to_string());
        }
        let head = resolve_git_commit(root, "HEAD")?;
        let main = resolve_git_commit(root, "refs/remotes/origin/main")?;
        if head != main {
            return Err("release source is not the current origin/main commit".to_string());
        }
    }

    Ok(ReleaseSource {
        channel,
        version: package_version,
    })
}

fn package_json_release_version(root: &Path) -> Result<String, String> {
    let bytes = read_root_release_file(root, "package.json", "package manifest")?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "package manifest is malformed".to_string())?;
    bounded_release_version(manifest.get("version"), "package manifest")
}

fn cargo_manifest_release_version(root: &Path) -> Result<String, String> {
    let bytes = read_root_release_file(root, "Cargo.toml", "Cargo manifest")?;
    let contents =
        String::from_utf8(bytes).map_err(|_| "Cargo manifest is not valid UTF-8".to_string())?;
    let mut in_package = false;
    let mut version = None;
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        if version.is_some() {
            return Err("Cargo package version is duplicated".to_string());
        }
        version = Some(parse_toml_release_string(value, "Cargo package version")?);
    }
    let version = version.ok_or_else(|| "Cargo package version is unavailable".to_string())?;
    if !is_bounded_version(&version) {
        return Err("Cargo manifest version is malformed".to_string());
    }
    Ok(version)
}

fn cargo_lock_release_version(root: &Path) -> Result<String, String> {
    let bytes = read_root_release_file(root, "Cargo.lock", "Cargo lockfile")?;
    let contents =
        String::from_utf8(bytes).map_err(|_| "Cargo lockfile is not valid UTF-8".to_string())?;
    let mut matches = Vec::new();
    for package in contents.split("[[package]]").skip(1) {
        let name = parse_toml_package_field(package, "name", "Cargo lockfile package name")?;
        if name.as_deref() != Some("repogrammar") {
            continue;
        }
        let version =
            parse_toml_package_field(package, "version", "Cargo lockfile package version")?
                .ok_or_else(|| "Cargo lockfile RepoGrammar version is unavailable".to_string())?;
        matches.push(version);
    }
    if matches.len() != 1 {
        return Err("Cargo lockfile must contain exactly one RepoGrammar package".to_string());
    }
    let version = matches.remove(0);
    if !is_bounded_version(&version) {
        return Err("Cargo lockfile version is malformed".to_string());
    }
    Ok(version)
}

fn read_root_release_file(root: &Path, name: &str, label: &str) -> Result<Vec<u8>, String> {
    let path = root.join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|_| format!("{label} is unavailable"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RELEASE_MANIFEST_BYTES
    {
        return Err(format!("{label} must be a bounded regular file"));
    }
    fs::read(path).map_err(|_| format!("{label} is unavailable"))
}

fn parse_toml_package_field(
    package: &str,
    field: &str,
    label: &str,
) -> Result<Option<String>, String> {
    let mut found = None;
    for raw_line in package.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            break;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != field {
            continue;
        }
        if found.is_some() {
            return Err(format!("{label} is duplicated"));
        }
        found = Some(parse_toml_release_string(value, label)?);
    }
    Ok(found)
}

fn parse_toml_release_string(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(format!("{label} is malformed"));
    }
    let value = &value[1..value.len() - 1];
    if value
        .chars()
        .any(|character| matches!(character, '"' | '\\' | '\n' | '\r'))
    {
        return Err(format!("{label} is malformed"));
    }
    Ok(value.to_string())
}

fn bounded_release_version(
    value: Option<&serde_json::Value>,
    label: &str,
) -> Result<String, String> {
    let version = value
        .and_then(serde_json::Value::as_str)
        .filter(|version| is_bounded_version(version))
        .ok_or_else(|| format!("{label} version is malformed"))?;
    Ok(version.to_string())
}

fn resolve_git_commit(root: &Path, revision: &str) -> Result<String, String> {
    let revision = format!("{revision}^{{commit}}");
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &revision])
        .current_dir(root)
        .output()
        .map_err(|_| "release source revision could not be resolved".to_string())?;
    if !output.status.success() {
        return Err("release source revision is unavailable".to_string());
    }
    let commit = String::from_utf8(output.stdout)
        .map_err(|_| "release source revision is malformed".to_string())?;
    let commit = commit.trim();
    if !is_git_commit_id(commit) {
        return Err("release source revision is malformed".to_string());
    }
    Ok(commit.to_string())
}

fn is_git_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseDistTagAction {
    Preview(PreviewDistTagAction),
    StableVerified,
}

impl ReleaseDistTagAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preview(action) => action.as_str(),
            Self::StableVerified => "stable_latest_verified",
        }
    }
}

fn release_dist_tag_action(
    version: &str,
    preview: &str,
    latest: &str,
    tags_json: &str,
    versions_json: &str,
) -> Result<ReleaseDistTagAction, &'static str> {
    let channel = release_channel(version)?;
    if tags_json.len() > 4096 {
        return Err("dist-tag inventory is malformed");
    }
    let tags: serde_json::Value =
        serde_json::from_str(tags_json).map_err(|_| "dist-tag inventory is malformed")?;
    let tags = tags.as_object().ok_or("dist-tag inventory is not exact")?;
    let expected_tag_count = if latest.is_empty() { 1 } else { 2 };
    if tags.len() != expected_tag_count
        || !tags.contains_key("preview")
        || (!latest.is_empty() && !tags.contains_key("latest"))
        || tags.keys().any(|key| key != "latest" && key != "preview")
    {
        return Err("dist-tag inventory is not exact");
    }
    if tags["preview"].as_str() != Some(preview)
        || (!latest.is_empty() && tags["latest"].as_str() != Some(latest))
    {
        return Err("dist-tag inventory does not match the classified values");
    }
    match channel {
        ReleaseChannel::Preview => preview_dist_tag_action(version, preview, latest, versions_json)
            .map(ReleaseDistTagAction::Preview),
        ReleaseChannel::Stable => {
            if version != STABLE_RELEASE_VERSION {
                return Err("stable release policy is not registered for this version");
            }
            if latest != version {
                return Err("latest does not match the stable manifest version");
            }
            if preview != STABLE_PREVIEW_VERSION {
                return Err("preview does not match the required stable predecessor");
            }
            let published_versions = parse_published_versions(versions_json)?;
            if published_versions
                .iter()
                .any(|published| FAILED_STABLE_RELEASE_VERSIONS.contains(&published.as_str()))
            {
                return Err("failed stable candidate versions must not be published");
            }
            if !published_versions
                .iter()
                .any(|published| published == version)
            {
                return Err("stable manifest version is not published");
            }
            if !published_versions
                .iter()
                .any(|published| published == preview)
            {
                return Err("preview does not reference a published version");
            }
            Ok(ReleaseDistTagAction::StableVerified)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewDistTagAction {
    NoTag,
    AllowPrereleaseWithoutStable,
    PreserveStable,
}

impl PreviewDistTagAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoTag => "no_latest",
            Self::AllowPrereleaseWithoutStable => "allow_prerelease_latest_without_stable",
            Self::PreserveStable => "preserve_stable_latest",
        }
    }
}

fn preview_dist_tag_action(
    version: &str,
    preview: &str,
    latest: &str,
    versions_json: &str,
) -> Result<PreviewDistTagAction, &'static str> {
    if !is_bounded_version(version) || !has_prerelease(version) {
        return Err("manifest version is not a bounded prerelease");
    }
    if preview != version {
        return Err("preview does not match the manifest version");
    }
    let published_versions = parse_published_versions(versions_json)?;
    if !published_versions
        .iter()
        .any(|published| published == version)
    {
        return Err("manifest version is not published");
    }
    if latest.is_empty() {
        return Ok(PreviewDistTagAction::NoTag);
    }
    if !is_bounded_version(latest) {
        return Err("latest is malformed");
    }
    if !published_versions
        .iter()
        .any(|published| published == latest)
    {
        return Err("latest does not reference a published version");
    }
    if has_prerelease(latest) {
        if published_versions
            .iter()
            .any(|published| !has_prerelease(published))
        {
            Err("latest is a prerelease while stable versions exist")
        } else {
            Ok(PreviewDistTagAction::AllowPrereleaseWithoutStable)
        }
    } else {
        Ok(PreviewDistTagAction::PreserveStable)
    }
}

fn parse_published_versions(versions_json: &str) -> Result<Vec<String>, &'static str> {
    if versions_json.is_empty() || versions_json.len() > MAX_PUBLISHED_VERSIONS_JSON_BYTES {
        return Err("published versions are unavailable or too large");
    }
    let versions: Vec<String> =
        serde_json::from_str(versions_json).map_err(|_| "published versions are malformed")?;
    if versions.is_empty() || versions.len() > MAX_PUBLISHED_VERSIONS {
        return Err("published version count is outside the supported bound");
    }
    if versions.iter().any(|version| !is_bounded_version(version)) {
        return Err("published versions contain a malformed version");
    }
    Ok(versions)
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

    let instruction_file = smoke.home.join("AGENTS.md");
    let instruction_file_arg = instruction_file
        .to_str()
        .ok_or_else(|| "isolated instruction path was not UTF-8".to_string())?;
    if smoke.project.join(".repogrammar").exists()
        || instruction_file.exists()
        || smoke.home.join("CLAUDE.md").exists()
    {
        return Err("instruction smoke isolation was not clean".to_string());
    }
    let instruction_sync = smoke.run_json(
        "instruction sync",
        &[
            "instructions",
            "sync",
            "--file",
            instruction_file_arg,
            "--yes",
            "--json",
        ],
    )?;
    if instruction_sync["state_after"] != "current"
        || instruction_sync["expected_content_version"] != MANAGED_INSTRUCTION_VERSION
    {
        return Err("instruction sync did not activate the current managed contract".to_string());
    }
    let instruction_contents = fs::read_to_string(&instruction_file)
        .map_err(|_| "managed instruction artifact was unavailable".to_string())?;
    if instruction_contents != format!("{}\n", managed_instruction_block())
        || smoke.home.join("CLAUDE.md").exists()
        || smoke.project.join(".repogrammar").exists()
    {
        return Err("instruction sync modified state outside its explicit file".to_string());
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

fn smoke_npm_package(
    repository_root: &Path,
    tarball: &str,
    expected_version: &str,
) -> Result<String, String> {
    if !is_bounded_version(expected_version) {
        return Err("expected version is invalid".to_string());
    }
    let tarball = regular_input_file(repository_root, tarball, "npm package candidate")?;
    let metadata =
        fs::metadata(&tarball).map_err(|_| "npm package candidate is unavailable".to_string())?;
    if metadata.len() == 0 || metadata.len() > MAX_NPM_CANDIDATE_BYTES {
        return Err("npm package candidate size is outside the supported bound".to_string());
    }
    let expected_filename = format!("sioyooo-repogrammar-{expected_version}.tgz");
    if tarball.file_name().and_then(OsStr::to_str) != Some(expected_filename.as_str()) {
        return Err(
            "npm package candidate filename does not match the release version".to_string(),
        );
    }

    let scratch = unique_smoke_root().with_file_name(format!(
        "repogrammar-npm-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&scratch)
        .map_err(|_| "could not create isolated npm package smoke root".to_string())?;
    let result = smoke_npm_package_inner(&tarball, expected_version, &expected_filename, &scratch);
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn verify_npm_pack_evidence(
    repository_root: &Path,
    pack_json: &str,
    candidate_manifest: &str,
    expected_version: &str,
) -> Result<(), String> {
    if !is_bounded_version(expected_version) {
        return Err("expected version is invalid".to_string());
    }
    let pack_json = regular_input_file(repository_root, pack_json, "npm pack metadata")?;
    let candidate_manifest = regular_input_file(
        repository_root,
        candidate_manifest,
        "npm candidate evidence manifest",
    )?;
    let pack: serde_json::Value = serde_json::from_slice(
        &fs::read(pack_json).map_err(|_| "npm pack metadata is unavailable".to_string())?,
    )
    .map_err(|_| "npm pack metadata is malformed".to_string())?;
    let entries = pack
        .as_array()
        .filter(|entries| entries.len() == 1)
        .ok_or_else(|| "npm pack metadata must contain exactly one candidate".to_string())?;
    let entry = &entries[0];
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(candidate_manifest)
            .map_err(|_| "npm candidate evidence manifest is unavailable".to_string())?,
    )
    .map_err(|_| "npm candidate evidence manifest is malformed".to_string())?;
    let filename = format!("sioyooo-repogrammar-{expected_version}.tgz");
    if entry["filename"] != filename
        || entry["integrity"] != manifest["integrity"]
        || manifest["schema_version"] != 1
        || manifest["package_name"] != NPM_PACKAGE_NAME
        || manifest["version"] != expected_version
        || manifest["filename"] != filename
        || manifest["offline_install_smoke"] != "passed"
        || manifest["local_release_asset_smoke"] != "passed"
    {
        return Err("npm pack metadata and candidate evidence do not agree".to_string());
    }
    let mut packed_files = entry["files"]
        .as_array()
        .ok_or_else(|| "npm pack file metadata is unavailable".to_string())?
        .iter()
        .map(|file| {
            file["path"]
                .as_str()
                .map(|path| format!("package/{path}"))
                .ok_or_else(|| "npm pack file metadata is malformed".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    packed_files.sort();
    let mut expected_files = NPM_PACKAGE_FILES.to_vec();
    expected_files.sort();
    if packed_files.iter().map(String::as_str).collect::<Vec<_>>() != expected_files
        || manifest["files"]
            != serde_json::Value::Array(
                NPM_PACKAGE_FILES
                    .iter()
                    .map(|file| serde_json::Value::String((*file).to_string()))
                    .collect(),
            )
    {
        return Err("npm pack file metadata is not the exact release allowlist".to_string());
    }
    Ok(())
}

fn verify_stable_release_evidence(
    repository_root: &Path,
    evidence_dir: &str,
) -> Result<(), String> {
    let directory = PathBuf::from(evidence_dir);
    let directory = if directory.is_absolute() {
        directory
    } else {
        repository_root.join(directory)
    };
    let metadata = fs::symlink_metadata(&directory)
        .map_err(|_| "release evidence directory is unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("release evidence directory must be a regular directory".to_string());
    }

    let release = read_evidence_json(&directory, "github-release.json")?;
    if release["tag_name"] != "v0.2.2"
        || release["draft"] != false
        || release["prerelease"] != false
        || release["immutable"] != true
    {
        return Err("GitHub stable release state is not final and immutable".to_string());
    }
    let asset_records = release["assets"]
        .as_array()
        .ok_or_else(|| "GitHub release asset inventory is unavailable".to_string())?
        .iter()
        .map(|asset| {
            let name = asset["name"]
                .as_str()
                .ok_or_else(|| "GitHub release asset inventory is malformed".to_string())?;
            let digest = asset["digest"]
                .as_str()
                .ok_or_else(|| "GitHub release asset metadata is incomplete".to_string())?;
            if asset["state"] != "uploaded" || !is_sha256_sri(digest) {
                return Err("GitHub release asset metadata is incomplete".to_string());
            }
            Ok((name.to_string(), digest.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut asset_names = asset_records
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    asset_names.sort();
    let mut expected_asset_names = STABLE_RELEASE_ASSETS.to_vec();
    expected_asset_names.sort();
    if asset_names.iter().map(String::as_str).collect::<Vec<_>>() != expected_asset_names {
        return Err("GitHub release asset inventory is not exact".to_string());
    }
    let release_attestation = read_evidence_json(&directory, "github-release-attestation.json")?;
    if release_attestation.is_null() {
        return Err("GitHub release attestation evidence is unavailable".to_string());
    }

    let assets = directory.join("github-assets");
    let assets_metadata = fs::symlink_metadata(&assets)
        .map_err(|_| "downloaded GitHub assets are unavailable".to_string())?;
    if assets_metadata.file_type().is_symlink() || !assets_metadata.is_dir() {
        return Err("downloaded GitHub assets must be a regular directory".to_string());
    }
    let actual_files = fs::read_dir(&assets)
        .map_err(|_| "downloaded GitHub assets are unavailable".to_string())?
        .map(|entry| {
            entry
                .map_err(|_| "downloaded GitHub asset inventory failed".to_string())
                .and_then(|entry| {
                    let metadata = entry
                        .file_type()
                        .map_err(|_| "downloaded GitHub asset metadata failed".to_string())?;
                    if !metadata.is_file() || metadata.is_symlink() {
                        return Err("downloaded GitHub assets must be regular files".to_string());
                    }
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| "downloaded GitHub asset name is invalid".to_string())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut actual_files = actual_files;
    actual_files.sort();
    if actual_files.iter().map(String::as_str).collect::<Vec<_>>() != expected_asset_names {
        return Err("downloaded GitHub asset inventory is not exact".to_string());
    }
    for asset in STABLE_RELEASE_ASSETS {
        let attestation_name = format!("asset-attestation-{asset}.json");
        if read_evidence_json(&directory, &attestation_name)?.is_null() {
            return Err("GitHub asset attestation evidence is unavailable".to_string());
        }
        let expected_digest = asset_records
            .iter()
            .find_map(|(name, digest)| (name == asset).then_some(digest.as_str()))
            .ok_or_else(|| "GitHub release asset inventory is not exact".to_string())?;
        let actual_digest = sha256_file_sri(&assets.join(asset))?;
        if actual_digest != expected_digest {
            return Err(
                "downloaded GitHub asset digest does not match release metadata".to_string(),
            );
        }
    }
    for checksum in STABLE_RELEASE_ASSETS
        .iter()
        .filter(|asset| asset.ends_with(".sha256"))
    {
        verify_sha256_sidecar(&assets, checksum)?;
    }

    let retained_original = read_evidence_json(&assets, "npm-candidate-manifest.json")?;
    let retained = read_evidence_json(&directory, "retained-candidate-manifest.json")?;
    let public = read_evidence_json(&directory, "public-candidate-manifest.json")?;
    if retained_original != retained || retained != public {
        return Err("retained and public npm candidate evidence differ".to_string());
    }
    validate_npm_candidate_manifest(&retained, STABLE_RELEASE_VERSION)?;
    verify_npm_pack_evidence(
        &directory,
        "public-npm-pack.json",
        "public-candidate-manifest.json",
        STABLE_RELEASE_VERSION,
    )?;
    let registry_integrity = read_evidence_text(&directory, "npm-registry-integrity.txt")?;
    if retained["integrity"].as_str() != Some(registry_integrity.trim()) {
        return Err("npm registry integrity does not match the retained candidate".to_string());
    }

    let tags = read_evidence_json(&directory, "npm-tags.json")?;
    let tag_object = tags
        .as_object()
        .filter(|tags| {
            tags.len() == 2 && tags.contains_key("latest") && tags.contains_key("preview")
        })
        .ok_or_else(|| "npm stable and preview dist-tags are not exact".to_string())?;
    let versions = read_evidence_json(&directory, "npm-versions.json")?;
    let preview = tag_object["preview"].as_str().unwrap_or_default();
    let latest = tag_object["latest"].as_str().unwrap_or_default();
    let versions_json = serde_json::to_string(&versions)
        .map_err(|_| "npm published-version evidence is malformed".to_string())?;
    let tags_json = serde_json::to_string(&tags)
        .map_err(|_| "npm dist-tag evidence is malformed".to_string())?;
    if release_dist_tag_action(
        STABLE_RELEASE_VERSION,
        preview,
        latest,
        &tags_json,
        &versions_json,
    ) != Ok(ReleaseDistTagAction::StableVerified)
    {
        return Err("npm stable and preview dist-tags are not exact".to_string());
    }

    let candidate_run = read_evidence_json(&directory, "candidate-run.json")?;
    let candidate_run_keys = [
        "conclusion",
        "event",
        "head_branch",
        "head_sha",
        "name",
        "path",
        "run_attempt",
        "run_id",
    ];
    candidate_run
        .as_object()
        .filter(|run| {
            run.len() == candidate_run_keys.len()
                && candidate_run_keys.iter().all(|key| run.contains_key(*key))
        })
        .ok_or_else(|| "candidate run evidence fields are not exact".to_string())?;
    let expected_sha = read_evidence_text(&directory, "expected-head-sha.txt")?;
    let expected_sha = expected_sha.trim();
    if !is_git_commit_id(expected_sha) {
        return Err("stable release head commit evidence is malformed".to_string());
    }
    let run_id = candidate_run["run_id"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| "candidate run identity is malformed".to_string())?;
    let run_attempt = candidate_run["run_attempt"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| "candidate run identity is malformed".to_string())?;
    if candidate_run["name"] != "Release"
        || candidate_run["event"] != "push"
        || candidate_run["conclusion"] != "success"
        || candidate_run["head_branch"] != "v0.2.2"
        || candidate_run["path"] != RELEASE_WORKFLOW_PATH
        || candidate_run["head_sha"].as_str() != Some(expected_sha)
    {
        return Err("candidate run does not match the stable tag commit".to_string());
    }

    let audit = read_evidence_json(&directory, "npm-audit-signatures.json")?;
    let manifest_sha512 = retained["sha512"]
        .as_str()
        .ok_or_else(|| "npm candidate SHA-512 evidence is unavailable".to_string())?;
    verify_npm_provenance(&audit, manifest_sha512, expected_sha, run_id, run_attempt)?;

    for (name, expected) in [
        ("pinned-version.txt", "repogrammar 0.2.2"),
        ("latest-version.txt", "repogrammar 0.2.2"),
        ("preview-version.txt", "repogrammar 0.2.0-preview.0"),
        ("public-installer-version.txt", "repogrammar 0.2.2"),
    ] {
        if read_evidence_text(&directory, name)?.trim() != expected {
            return Err("public npm version smoke did not match the expected channel".to_string());
        }
    }
    if read_evidence_text(&directory, "public-native-smoke.txt")?.trim()
        != "packaged artifact smoke passed"
    {
        return Err("public native product smoke did not pass".to_string());
    }
    for setup_name in ["pinned-setup.json", "latest-setup.json"] {
        validate_live_setup(&read_evidence_json(&directory, setup_name)?)?;
    }
    let dry_run_path = directory.join("setup.json");
    if dry_run_path.exists() {
        let setup = read_evidence_json(&directory, "setup.json")?;
        if setup["status"] != "dry_run"
            || setup["repository_index_ready"] != false
            || setup["agent_query_ready"] != false
            || !setup["suggested_question"].is_null()
        {
            return Err("public stable setup dry-run evidence is not truthful".to_string());
        }
    }
    Ok(())
}

fn validate_npm_candidate_manifest(
    manifest: &serde_json::Value,
    expected_version: &str,
) -> Result<(), String> {
    let expected_keys = [
        "filename",
        "files",
        "integrity",
        "local_release_asset_smoke",
        "offline_install_smoke",
        "package_name",
        "schema_version",
        "sha512",
        "version",
    ];
    manifest
        .as_object()
        .filter(|manifest| {
            manifest.len() == expected_keys.len()
                && expected_keys.iter().all(|key| manifest.contains_key(*key))
        })
        .ok_or_else(|| "npm candidate evidence manifest is incomplete".to_string())?;
    let expected_filename = format!("sioyooo-repogrammar-{expected_version}.tgz");
    let expected_files = serde_json::Value::Array(
        NPM_PACKAGE_FILES
            .iter()
            .map(|file| serde_json::Value::String((*file).to_string()))
            .collect(),
    );
    let sha512 = manifest["sha512"].as_str().unwrap_or_default();
    let integrity = manifest["integrity"].as_str().unwrap_or_default();
    let integrity_bytes = integrity
        .strip_prefix("sha512-")
        .ok_or_else(|| "npm candidate evidence manifest is incomplete".to_string())
        .and_then(|encoded| {
            base64_decode_bounded(encoded)
                .map_err(|_| "npm candidate evidence manifest is incomplete".to_string())
        })?;
    if manifest["schema_version"] != 1
        || manifest["package_name"] != NPM_PACKAGE_NAME
        || manifest["version"] != expected_version
        || manifest["filename"] != expected_filename
        || manifest["files"] != expected_files
        || sha512.len() != 128
        || !sha512
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || integrity_bytes.len() != 64
        || hex_digest(&integrity_bytes) != sha512
        || manifest["offline_install_smoke"] != "passed"
        || manifest["local_release_asset_smoke"] != "passed"
    {
        return Err("npm candidate evidence manifest is incomplete".to_string());
    }
    Ok(())
}

fn validate_live_setup(setup: &serde_json::Value) -> Result<(), String> {
    if setup["product_self_test_state"] != "passed"
        || setup["repository_index_ready"] != true
        || setup["agent_query_ready"] != false
        || !setup["suggested_question"].is_null()
    {
        return Err("public live setup evidence is not truthful".to_string());
    }
    Ok(())
}

fn verify_npm_provenance(
    audit: &serde_json::Value,
    candidate_sha512: &str,
    expected_sha: &str,
    run_id: u64,
    run_attempt: u64,
) -> Result<(), String> {
    if audit["invalid"]
        .as_array()
        .is_none_or(|entries| !entries.is_empty())
        || audit["missing"]
            .as_array()
            .is_none_or(|entries| !entries.is_empty())
    {
        return Err("npm signature verification reported missing or invalid entries".to_string());
    }
    let verified = audit["verified"]
        .as_array()
        .filter(|entries| entries.len() == 1)
        .ok_or_else(|| "npm provenance must contain exactly one verified package".to_string())?;
    let entry = &verified[0];
    if entry["name"] != NPM_PACKAGE_NAME
        || entry["version"] != STABLE_RELEASE_VERSION
        || entry["registry"] != NPM_REGISTRY_URL
    {
        return Err("npm provenance package identity is not exact".to_string());
    }
    let attestations = entry["attestations"]
        .as_object()
        .ok_or_else(|| "npm provenance predicate inventory is unavailable".to_string())?;
    if attestations["url"]
        .as_str()
        .is_none_or(|url| url.is_empty() || url.len() > 2048)
        || attestations["provenance"]["predicateType"] != SLSA_PROVENANCE_V1
    {
        return Err("npm provenance predicate is not exact".to_string());
    }

    let bundles = entry["attestationBundles"]
        .as_array()
        .ok_or_else(|| "npm provenance bundle inventory is unavailable".to_string())?;
    if bundles.len() != 2
        || bundles
            .iter()
            .filter(|bundle| bundle["predicateType"] == NPM_PUBLISH_ATTESTATION_V1)
            .count()
            != 1
    {
        return Err("npm provenance bundle inventory is not exact".to_string());
    }
    let mut slsa_bundles = bundles
        .iter()
        .filter(|candidate| candidate["predicateType"] == SLSA_PROVENANCE_V1);
    let slsa_bundle = slsa_bundles
        .next()
        .ok_or_else(|| "npm provenance SLSA v1 statement is unavailable".to_string())?;
    if slsa_bundles.next().is_some() {
        return Err("npm provenance contains multiple SLSA v1 statements".to_string());
    }
    let envelope = &slsa_bundle["bundle"]["dsseEnvelope"];
    if envelope["payloadType"] != IN_TOTO_PAYLOAD_TYPE {
        return Err("npm provenance DSSE payload type is not exact".to_string());
    }
    let payload = envelope["payload"]
        .as_str()
        .ok_or_else(|| "npm provenance DSSE payload is unavailable".to_string())?;
    let decoded = base64_decode_bounded(payload)?;
    let provenance: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|_| "npm provenance DSSE payload is malformed".to_string())?;
    if provenance["predicateType"] != SLSA_PROVENANCE_V1 {
        return Err("npm provenance declaration and payload do not agree".to_string());
    }
    validate_slsa_provenance(
        &provenance,
        candidate_sha512,
        expected_sha,
        run_id,
        run_attempt,
    )
}

fn validate_slsa_provenance(
    statement: &serde_json::Value,
    candidate_sha512: &str,
    expected_sha: &str,
    run_id: u64,
    run_attempt: u64,
) -> Result<(), String> {
    let expected_purl = format!("pkg:npm/%40sioyooo/repogrammar@{STABLE_RELEASE_VERSION}");
    let subjects = statement["subject"]
        .as_array()
        .filter(|subjects| subjects.len() == 1)
        .ok_or_else(|| "npm provenance subject is not exact".to_string())?;
    let subject = &subjects[0];
    let build_definition = &statement["predicate"]["buildDefinition"];
    let workflow = &build_definition["externalParameters"]["workflow"];
    let github = &build_definition["internalParameters"]["github"];
    let dependencies = build_definition["resolvedDependencies"]
        .as_array()
        .filter(|dependencies| dependencies.len() == 1)
        .ok_or_else(|| "npm provenance resolved dependency is not exact".to_string())?;
    let run_details = &statement["predicate"]["runDetails"];
    let expected_invocation =
        format!("{RELEASE_REPOSITORY_URL}/actions/runs/{run_id}/attempts/{run_attempt}");
    if statement["_type"] != IN_TOTO_STATEMENT_V1
        || statement["predicateType"] != SLSA_PROVENANCE_V1
        || subject["name"] != expected_purl
        || subject["digest"]["sha512"] != candidate_sha512
        || build_definition["buildType"] != GITHUB_WORKFLOW_BUILD_TYPE_V1
        || workflow["repository"] != RELEASE_REPOSITORY_URL
        || workflow["path"] != RELEASE_WORKFLOW_PATH
        || workflow["ref"] != "refs/tags/v0.2.2"
        || github["event_name"] != "push"
        || dependencies[0]["uri"] != RELEASE_DEPENDENCY_URI
        || dependencies[0]["digest"]["gitCommit"] != expected_sha
        || run_details["builder"]["id"] != GITHUB_HOSTED_BUILDER
        || run_details["metadata"]["invocationId"] != expected_invocation
    {
        return Err(
            "npm provenance statement does not match the stable release policy".to_string(),
        );
    }
    Ok(())
}

fn is_sha256_sri(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn sha256_file_sri(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "downloaded GitHub asset is unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("downloaded GitHub asset must be a regular file".to_string());
    }
    let mut file =
        fs::File::open(path).map_err(|_| "downloaded GitHub asset is unavailable".to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "downloaded GitHub asset could not be read".to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex_digest(&digest.finalize())))
}

fn base64_decode_bounded(encoded: &str) -> Result<Vec<u8>, String> {
    if encoded.is_empty() || encoded.len() > MAX_DSSE_PAYLOAD_BASE64_BYTES {
        return Err("npm provenance DSSE payload is outside the supported bound".to_string());
    }
    let bytes = encoded.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("npm provenance DSSE payload is malformed".to_string());
    }

    let padding = if bytes.ends_with(b"==") {
        2
    } else if bytes.ends_with(b"=") {
        1
    } else {
        0
    };
    if bytes[..bytes.len() - padding].contains(&b'=') {
        return Err("npm provenance DSSE payload is malformed".to_string());
    }

    let output_len = bytes.len() / 4 * 3 - padding;
    if output_len > MAX_DSSE_PAYLOAD_BASE64_BYTES {
        return Err("npm provenance DSSE payload is outside the supported bound".to_string());
    }
    let mut decoded = Vec::with_capacity(output_len);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let a = base64_value(chunk[0]);
        let b = base64_value(chunk[1]);
        let c = base64_value(chunk[2]);
        let d = base64_value(chunk[3]);
        let values = match (a, b, c, d, last, padding) {
            (Some(a), Some(b), Some(c), Some(d), _, _) => (a, b, c, d, 3),
            (Some(a), Some(b), Some(c), None, true, 1) if chunk[3] == b'=' && c & 0x03 == 0 => {
                (a, b, c, 0, 2)
            }
            (Some(a), Some(b), None, None, true, 2)
                if chunk[2] == b'=' && chunk[3] == b'=' && b & 0x0f == 0 =>
            {
                (a, b, 0, 0, 1)
            }
            _ => return Err("npm provenance DSSE payload is malformed".to_string()),
        };
        decoded.push((values.0 << 2) | (values.1 >> 4));
        if values.4 > 1 {
            decoded.push((values.1 << 4) | (values.2 >> 2));
        }
        if values.4 > 2 {
            decoded.push((values.2 << 6) | values.3);
        }
    }
    Ok(decoded)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn read_evidence_json(directory: &Path, name: &str) -> Result<serde_json::Value, String> {
    let contents = read_evidence_bytes(directory, name, 16 * 1024 * 1024)?;
    serde_json::from_slice(&contents).map_err(|_| "release JSON evidence is malformed".to_string())
}

fn read_evidence_text(directory: &Path, name: &str) -> Result<String, String> {
    String::from_utf8(read_evidence_bytes(directory, name, 1024 * 1024)?)
        .map_err(|_| "release text evidence is not UTF-8".to_string())
}

fn read_evidence_bytes(directory: &Path, name: &str, maximum: u64) -> Result<Vec<u8>, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("release evidence name is invalid".to_string());
    }
    let path = directory.join(name);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "required release evidence is unavailable".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > maximum
    {
        return Err("required release evidence is not a bounded regular file".to_string());
    }
    fs::read(path).map_err(|_| "required release evidence is unavailable".to_string())
}

fn verify_sha256_sidecar(directory: &Path, checksum_name: &str) -> Result<(), String> {
    let checksum = fs::read_to_string(directory.join(checksum_name))
        .map_err(|_| "release checksum is unavailable".to_string())?;
    let expected = checksum
        .split_whitespace()
        .next()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| "release checksum is malformed".to_string())?;
    let asset_name = checksum_name
        .strip_suffix(".sha256")
        .ok_or_else(|| "release checksum filename is malformed".to_string())?;
    let path = directory.join(asset_name);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "checksummed release asset is unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("checksummed release asset must be a regular file".to_string());
    }
    let mut file =
        fs::File::open(path).map_err(|_| "checksummed release asset is unavailable".to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "checksummed release asset could not be read".to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if hex_digest(&digest.finalize()) != expected.to_ascii_lowercase() {
        return Err("release asset checksum does not match".to_string());
    }
    Ok(())
}

fn smoke_npm_package_inner(
    tarball: &Path,
    expected_version: &str,
    expected_filename: &str,
    scratch: &Path,
) -> Result<String, String> {
    let tar = executable_on_path("tar").ok_or_else(|| "tar is unavailable".to_string())?;
    let npm = executable_on_path("npm").ok_or_else(|| "npm is unavailable".to_string())?;
    let path = env::var_os("PATH").ok_or_else(|| "tool PATH is unavailable".to_string())?;

    let listing = Command::new(&tar)
        .args(["-tzf"])
        .arg(tarball)
        .output()
        .map_err(|_| "npm package file inventory could not execute".to_string())?;
    if !listing.status.success() {
        return Err("npm package file inventory failed".to_string());
    }
    let listing = String::from_utf8(listing.stdout)
        .map_err(|_| "npm package file inventory was not UTF-8".to_string())?;
    let mut files = listing
        .lines()
        .map(|entry| entry.trim_start_matches("./").trim_end_matches('/'))
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    let mut expected_files = NPM_PACKAGE_FILES
        .iter()
        .map(|entry| (*entry).to_string())
        .collect::<Vec<_>>();
    expected_files.sort();
    if files != expected_files {
        return Err("npm package file inventory is not the exact release allowlist".to_string());
    }

    let unpacked = scratch.join("unpacked");
    fs::create_dir_all(&unpacked)
        .map_err(|_| "could not create isolated npm package extraction root".to_string())?;
    let extracted = Command::new(&tar)
        .args(["-xzf"])
        .arg(tarball)
        .arg("-C")
        .arg(&unpacked)
        .status()
        .map_err(|_| "npm package extraction could not execute".to_string())?;
    if !extracted.success() {
        return Err("npm package extraction failed".to_string());
    }
    for entry in NPM_PACKAGE_FILES {
        let metadata = fs::symlink_metadata(unpacked.join(entry))
            .map_err(|_| "npm package omitted an allowed file".to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("npm package contains a non-regular allowed path".to_string());
        }
    }

    let package_json = fs::read(unpacked.join("package/package.json"))
        .map_err(|_| "npm package manifest is unavailable".to_string())?;
    let package_json: serde_json::Value = serde_json::from_slice(&package_json)
        .map_err(|_| "npm package manifest is malformed".to_string())?;
    if package_json["name"] != NPM_PACKAGE_NAME
        || package_json["version"] != expected_version
        || package_json["bin"]["repogrammar"] != "src/npm/repogrammar.js"
    {
        return Err("npm package manifest does not match the release contract".to_string());
    }

    let home = scratch.join("home");
    let prefix = scratch.join("installed");
    let cache = scratch.join("npm-cache");
    fs::create_dir_all(&home).map_err(|_| "could not create isolated npm HOME".to_string())?;
    let installed = Command::new(npm)
        .env_clear()
        .env("PATH", &path)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("npm_config_cache", &cache)
        .env("npm_config_offline", "true")
        .env("npm_config_registry", "http://127.0.0.1:9")
        .args([
            "install",
            "--global",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--prefix",
        ])
        .arg(&prefix)
        .arg(tarball)
        .output()
        .map_err(|_| "offline npm package installation could not execute".to_string())?;
    if !installed.status.success() {
        return Err("offline npm package installation failed".to_string());
    }

    smoke_installed_npm_wrapper(&prefix, expected_version, scratch, &tar, &path)?;

    let bytes =
        fs::read(tarball).map_err(|_| "npm package candidate is unavailable".to_string())?;
    let sha512 = Sha512::digest(&bytes);
    let manifest = serde_json::json!({
        "schema_version": 1,
        "package_name": NPM_PACKAGE_NAME,
        "version": expected_version,
        "filename": expected_filename,
        "sha512": hex_digest(&sha512),
        "integrity": format!("sha512-{}", base64_encode(&sha512)),
        "files": expected_files,
        "offline_install_smoke": "passed",
        "local_release_asset_smoke": "passed"
    });
    serde_json::to_string_pretty(&manifest)
        .map_err(|_| "npm package evidence manifest could not be serialized".to_string())
}

#[cfg(unix)]
fn smoke_installed_npm_wrapper(
    prefix: &Path,
    expected_version: &str,
    scratch: &Path,
    tar: &Path,
    path: &OsStr,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let target = match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return Err("npm package smoke supports declared release hosts only".to_string()),
    };
    let payload = scratch.join("fake-release-payload");
    fs::create_dir_all(payload.join("workers/python"))
        .map_err(|_| "could not create local release payload".to_string())?;
    let binary = payload.join("repogrammar");
    fs::write(
        &binary,
        format!("#!/bin/sh\nprintf 'repogrammar {expected_version}\\n'\n"),
    )
    .map_err(|_| "could not create local release binary".to_string())?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
        .map_err(|_| "could not authorize local release binary".to_string())?;
    fs::write(
        payload.join("workers/python/worker.py"),
        b"# packaged worker\n",
    )
    .map_err(|_| "could not create local release worker".to_string())?;

    let release = scratch.join("fake-release");
    fs::create_dir_all(&release)
        .map_err(|_| "could not create local release directory".to_string())?;
    let artifact = format!("repogrammar-{target}.tar.gz");
    let archive = release.join(&artifact);
    let archived = Command::new(tar)
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(&payload)
        .args(["repogrammar", "workers"])
        .status()
        .map_err(|_| "local release archive could not execute".to_string())?;
    if !archived.success() {
        return Err("local release archive failed".to_string());
    }
    let archive_bytes =
        fs::read(&archive).map_err(|_| "local release archive is unavailable".to_string())?;
    let archive_sha256 = Sha256::digest(archive_bytes);
    fs::write(
        release.join(format!("{artifact}.sha256")),
        format!("{}  {artifact}\n", hex_digest(&archive_sha256)),
    )
    .map_err(|_| "could not create local release checksum".to_string())?;

    let launcher = prefix.join("bin/repogrammar");
    let output = Command::new(&launcher)
        .env_clear()
        .env("PATH", path)
        .env("HOME", scratch.join("wrapper-home"))
        .env("USERPROFILE", scratch.join("wrapper-home"))
        .env("REPOGRAMMAR_RELEASE_DIR", &release)
        .env("REPOGRAMMAR_NPM_CACHE_DIR", scratch.join("wrapper-cache"))
        .env("REPOGRAMMAR_VERSION", format!("v{expected_version}"))
        .arg("version")
        .output()
        .map_err(|_| "installed npm wrapper could not execute".to_string())?;
    if !output.status.success()
        || String::from_utf8(output.stdout)
            .map_err(|_| "installed npm wrapper output was not UTF-8".to_string())?
            .trim()
            != format!("repogrammar {expected_version}")
    {
        return Err("installed npm wrapper did not execute the local release asset".to_string());
    }
    Ok(())
}

#[cfg(not(unix))]
fn smoke_installed_npm_wrapper(
    _prefix: &Path,
    _expected_version: &str,
    _scratch: &Path,
    _tar: &Path,
    _path: &OsStr,
) -> Result<(), String> {
    Err("npm package smoke supports declared release hosts only".to_string())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(TABLE[(first >> 2) as usize] as char);
        encoded.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(third & 0x3f) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
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
    let finalizer_path = root.join(".github/workflows/stable-release-finalize.yml");
    let (Ok(release), Ok(reconcile), Ok(finalizer)) = (
        fs::read_to_string(&release_path),
        fs::read_to_string(&reconcile_path),
        fs::read_to_string(&finalizer_path),
    ) else {
        return;
    };

    if release.contains("npm publish")
        || release.contains("npm stage approve")
        || release.contains("npm stage reject")
        || release.contains("npm dist-tag add")
        || release.contains("npm dist-tag rm")
    {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "ReleasePublicationAuthority",
            "preview and stable may only stage through OIDC; CI must never publish directly, approve, reject, or mutate dist-tags",
        ));
    }

    if release_workflow_combines_gh_api_slurp_with_jq(&release) {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "ReleaseWorkflowRunnerCompatibility",
            "gh api must not combine --slurp with --jq; paginated release queries must filter each response page directly",
        ));
    }

    let package_job = workflow_job_section(&release, "package_npm");
    let preview_job = workflow_job_section(&release, "stage_npm_preview");
    let stable_job = workflow_job_section(&release, "stage_npm_stable");
    let classify_job = workflow_job_section(&release, "classify");
    let prepare_job = workflow_job_section(&release, "prepare_github_release");
    if prepare_job.is_none_or(|job| !release_workflow_has_draft_collision_guard(job)) {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "StableReleaseDraftCollisionContract",
            "draft creation must query every release page with the runner-compatible exact tag filter and fail when that tag already has a release or draft",
        ));
    }
    let exact_stage_command = STABLE_NPM_STAGE_COMMAND;
    let stable_global_markers = ["release-source", "draft: true"];
    let stable_job_markers = [
        "needs: [classify, package_npm, prepare_github_release]",
        "if: github.event_name == 'push' && github.ref_type == 'tag' && needs.classify.outputs.channel == 'stable'",
        "environment: npm-release",
        "id-token: write",
        "node-version: 24",
        "npm@11.18.0",
        "actions/download-artifact@v8",
        "name: npm-package-${{ needs.classify.outputs.version }}",
        "smoke-npm-package",
        "verify-npm-pack-evidence",
    ];
    let exact_stage_count = stable_job.map_or(0, |job| {
        job.lines()
            .filter(|line| line.trim() == exact_stage_command)
            .count()
    });
    let stable_has_forbidden_authority = stable_job.is_some_and(|job| {
        job.lines().any(|line| {
            let command = line.trim();
            (command.starts_with("npm pack ") || command.contains("$(npm pack "))
                || command.starts_with("npm publish ")
                || (command.contains("npm stage publish") && command != exact_stage_command)
        }) || job.contains("npm stage approve")
            || job.contains("npm stage reject")
            || job.contains("NODE_AUTH_TOKEN")
            || job.contains("NPM_TOKEN")
            || job.contains("npm \"${")
            || job.contains("npm '${")
            || job.contains("npm ${")
            || job.contains("npm $subcommand")
    });
    if stable_global_markers
        .iter()
        .any(|marker| !release.contains(marker))
        || exact_stage_count != 1
        || stable_has_forbidden_authority
        || stable_job.is_none_or(|job| {
            stable_job_markers
                .iter()
                .any(|marker| !job.contains(marker))
        })
    {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "StableReleaseStagingContract",
            "stable tags must consume the once-built npm candidate through one exact OIDC stage command and no alternate publication authority",
        ));
    }

    let release_source_markers = [
        "fetch-depth: 0",
        "id: release",
        "channel: ${{ steps.release.outputs.channel }}",
        "version: ${{ steps.release.outputs.version }}",
        "git fetch --no-tags origin main:refs/remotes/origin/main",
        "release-source --event-name \"${EVENT_NAME}\" --ref-name \"${PUSH_REF_NAME}\"",
    ];
    if classify_job.is_none_or(|job| {
        release_source_markers
            .iter()
            .any(|marker| !job.contains(marker))
    }) {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "ReleaseSourceContract",
            "release classification must export repo-guard's exact manifest and origin/main-bound channel and version",
        ));
    }

    let package_markers = [
        "package_npm:",
        "needs: [classify, verify]",
        "npm pack --json --ignore-scripts --pack-destination npm-candidate",
        "smoke-npm-package",
        "candidate-manifest.json",
        "verify-npm-pack-evidence",
        "actions/upload-artifact@v7",
        "name: npm-package-${{ needs.classify.outputs.version }}",
    ];
    if release
        .matches("npm pack --json --ignore-scripts --pack-destination npm-candidate")
        .count()
        != 1
        || !release.contains("package_installer:")
        || !release.contains("name: repogrammar-installer")
        || !release.contains("needs: [classify, build, package_installer, package_npm]")
        || package_job.is_none_or(|job| package_markers.iter().any(|marker| !job.contains(marker)))
        || preview_job.is_none_or(|job| {
            !job.contains("actions/download-artifact@v8")
                || !job.contains("environment: npm-release")
                || !job.contains("id-token: write")
                || !job.contains(PREVIEW_NPM_STAGE_PACKAGE_ASSIGNMENT)
                || !job.contains(
                    "npm stage publish \"${package_file}\" --access public --tag preview --provenance",
                )
                || job.lines().any(|line| {
                    let command = line.trim();
                    command.starts_with("npm pack ") || command.contains("$(npm pack ")
                })
                || job.contains("NODE_AUTH_TOKEN")
                || job.contains("NPM_TOKEN")
        })
    {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "NpmCandidateReuseContract",
            "build-only and tag runs must pack and smoke one npm candidate that preview and stable OIDC staging reuse without repacking or tokens",
        ));
    }

    let draft_asset_markers = [
        "npm-candidate-manifest.json",
        "test \"$(find release-assets -maxdepth 1 -type f | wc -l | tr -d ' ')\" = \"11\"",
        "Refuse an existing release or draft for this tag",
        "overwrite_files: false",
        "fail_on_unmatched_files: true",
    ];
    if prepare_job.is_none_or(|job| {
        draft_asset_markers
            .iter()
            .any(|marker| !job.contains(marker))
    }) {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "StableReleaseAssetContract",
            "the draft release must retain exactly eleven non-overwritable assets including the npm candidate manifest",
        ));
    }

    let manual_build_only_markers = [
        "workflow_dispatch:",
        "default: build-only",
        "if: github.event_name == 'push' && github.ref_type == 'tag'",
    ];
    if manual_build_only_markers
        .iter()
        .any(|marker| !release.contains(marker))
    {
        violations.push(GuardViolation::new(
            ".github/workflows/release.yml",
            "ManualReleaseDispatchContract",
            "manual dispatch must remain build-only while publication is limited to tag and release events",
        ));
    }

    let finalize_markers = [
        "workflow_dispatch:",
        "workflow_call:",
        "release-channel",
        "release-dist-tag-action",
        "versions --json",
        "--versions-json",
        "final_action=",
        "node-version: 24",
        "npm@11.18.0",
    ];
    if reconcile.contains("npm publish")
        || reconcile.contains("npm stage publish")
        || reconcile.contains("npm stage approve")
        || reconcile.contains("npm stage reject")
        || reconcile.contains("npm dist-tag add")
        || reconcile.contains("npm dist-tag rm")
        || finalize_markers
            .iter()
            .any(|marker| !reconcile.contains(marker))
    {
        violations.push(GuardViolation::new(
            ".github/workflows/npm-tag-reconcile.yml",
            "NpmReleaseFinalizeContract",
            "manual finalization must be read-only and classify complete preview or stable registry state",
        ));
    }

    let stable_finalize_markers = [
        "workflow_dispatch:",
        "candidate_run_id:",
        "candidate_run_attempt:",
        "permissions:",
        "contents: read",
        "actions: read",
        "gh release verify v0.2.2",
        "gh release verify-asset v0.2.2",
        "npm-candidate-manifest.json",
        "test \"$(find evidence/github-assets -maxdepth 1 -type f | wc -l | tr -d ' ')\" = \"11\"",
        "--jq '{run_id: .id, run_attempt: .run_attempt, name: .name, path: .path, event: .event, head_branch: .head_branch, head_sha: .head_sha, conclusion: .conclusion}'",
        "keys == [\"conclusion\", \"event\", \"head_branch\", \"head_sha\", \"name\", \"path\", \"run_attempt\", \"run_id\"]",
        "evidence/candidate-run.json",
        "evidence/public-npm-pack.json",
        "npm audit signatures --json --include-attestations",
        "--candidate-manifest evidence/retained-candidate-manifest.json",
        "smoke-npm-package",
        "evidence/public-native-smoke.txt",
        "evidence/public-installer-version.txt",
        "evidence/pinned-setup.json",
        "evidence/latest-setup.json",
        "verify-stable-release-evidence --evidence-dir evidence",
    ];
    let pack_verification_precedes_execution = finalizer
        .find("verify-npm-pack-evidence")
        .zip(finalizer.find("Smoke the verified public native archive"))
        .is_some_and(|(verification, execution)| verification < execution);
    if finalizer.contains("npm publish")
        || finalizer.contains("npm stage publish")
        || finalizer.contains("npm stage approve")
        || finalizer.contains("npm stage reject")
        || finalizer.contains("npm dist-tag add")
        || finalizer.contains("npm dist-tag rm")
        || finalizer.contains("id-token: write")
        || !pack_verification_precedes_execution
        || stable_finalize_markers
            .iter()
            .any(|marker| !finalizer.contains(marker))
    {
        violations.push(GuardViolation::new(
            ".github/workflows/stable-release-finalize.yml",
            "StableReleaseFinalizeContract",
            "stable finalization must be manually dispatched, read-only, and verify immutable GitHub assets plus npm integrity, provenance, tags, and public smoke paths",
        ));
    }
}

fn release_workflow_combines_gh_api_slurp_with_jq(workflow: &str) -> bool {
    let logical_lines = workflow.replace("\\\r\n", " ").replace("\\\n", " ");
    logical_lines
        .lines()
        .any(|line| line.contains("gh api") && line.contains("--slurp") && line.contains("--jq"))
}

fn release_workflow_has_draft_collision_guard(job: &str) -> bool {
    const EXACT_QUERY: &str = "existing_release_id=\"$(gh api --paginate \"repos/${GITHUB_REPOSITORY}/releases?per_page=100\" --jq '.[] | select(.tag_name == env.TAG_NAME) | .id')\"";
    let logical_job = job
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    logical_job.contains(EXACT_QUERY)
        && job.contains("if [ -n \"${existing_release_id}\" ]; then")
        && job.lines().any(|line| line.trim() == "exit 1")
}

fn workflow_job_section<'a>(workflow: &'a str, job: &str) -> Option<&'a str> {
    let marker = format!("\n  {job}:");
    let start = workflow.find(&marker)? + 1;
    let tail = &workflow[start..];
    let bytes = tail.as_bytes();
    let end = (1..bytes.len().saturating_sub(3))
        .find(|index| {
            bytes[*index] == b'\n'
                && bytes[*index + 1] == b' '
                && bytes[*index + 2] == b' '
                && bytes[*index + 3] != b' '
                && bytes[*index + 3] != b'\n'
        })
        .unwrap_or(tail.len());
    Some(&tail[..end])
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
    fn release_channel_is_typed_from_a_bounded_manifest_version() {
        assert_eq!(
            release_channel("0.2.0-preview.0"),
            Ok(ReleaseChannel::Preview)
        );
        assert_eq!(release_channel("0.2.0"), Ok(ReleaseChannel::Stable));
        assert_eq!(release_channel("0.2.0+build.7"), Ok(ReleaseChannel::Stable));
        assert_eq!(
            release_channel("02.0.0"),
            Err("manifest version is malformed")
        );
    }

    #[test]
    fn release_source_dispatch_requires_three_matching_manifests() {
        let root = TempRoot::new("release-source-dispatch");
        write_release_manifests(root.path(), "0.2.0", "0.2.0", "0.2.0");

        let source = release_source(root.path(), "workflow_dispatch", "main")
            .expect("matching dispatch source");
        assert_eq!(
            release_source_lines(&source),
            "channel=stable\nversion=0.2.0\n"
        );
        let command = release_source_command(root.path(), "workflow_dispatch", "main", None);
        assert_eq!(command.status, 0);
        assert_eq!(command.stdout, "channel=stable\nversion=0.2.0\n");
        assert!(command.stderr.is_empty());

        for versions in [
            ("0.2.1", "0.2.0", "0.2.0"),
            ("0.2.0", "0.2.1", "0.2.0"),
            ("0.2.0", "0.2.0", "0.2.1"),
        ] {
            write_release_manifests(root.path(), versions.0, versions.1, versions.2);
            let error = release_source(root.path(), "workflow_dispatch", "main")
                .expect_err("a version mismatch must fail closed");
            assert_eq!(error, "release manifest versions do not match");
        }
    }

    #[test]
    fn release_source_github_output_is_optional_bounded_and_exact() {
        let root = TempRoot::new("release-source-output");
        let source = ReleaseSource {
            channel: ReleaseChannel::Stable,
            version: "0.2.0".to_string(),
        };
        assert_eq!(append_release_source_github_output(&source, None), Ok(()));

        let output = root.path().join("github-output");
        write_file(output.clone(), b"existing=value\n");
        append_release_source_github_output(&source, Some(output.clone().into_os_string()))
            .expect("append release outputs");
        assert_eq!(
            fs::read_to_string(output).expect("read GitHub output"),
            "existing=value\nchannel=stable\nversion=0.2.0\n"
        );

        assert_eq!(
            append_release_source_github_output(
                &source,
                Some(root.path().join("missing").into_os_string())
            ),
            Err("GitHub output file is unavailable".to_string())
        );
        assert_eq!(
            append_release_source_github_output(&source, Some(root.path().as_os_str().to_owned())),
            Err("GitHub output file must be a bounded regular file".to_string())
        );
        assert_eq!(
            append_release_source_github_output(&source, Some(std::ffi::OsString::new())),
            Err("GitHub output path is malformed".to_string())
        );

        let oversized = root.path().join("oversized-output");
        write_file(
            oversized.clone(),
            vec![b'x'; MAX_GITHUB_OUTPUT_BYTES as usize].as_slice(),
        );
        assert_eq!(
            append_release_source_github_output(&source, Some(oversized.into_os_string())),
            Err("GitHub output file must be a bounded regular file".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn release_source_github_output_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("release-source-output-symlink");
        let source = ReleaseSource {
            channel: ReleaseChannel::Stable,
            version: "0.2.0".to_string(),
        };
        let target = root.path().join("target");
        let link = root.path().join("github-output");
        write_file(target.clone(), b"");
        symlink(target, &link).expect("create GitHub output symlink");
        assert_eq!(
            append_release_source_github_output(&source, Some(link.into_os_string())),
            Err("GitHub output file must be a bounded regular file".to_string())
        );
    }

    #[test]
    fn release_source_rejects_each_malformed_manifest() {
        let root = TempRoot::new("release-source-malformed");
        let malformed = [
            ("package.json", b"{".as_slice()),
            ("Cargo.toml", b"[package]\nversion = []\n".as_slice()),
            (
                "Cargo.lock",
                b"[[package]]\nname = \"repogrammar\"\nversion = []\n".as_slice(),
            ),
        ];
        for (name, contents) in malformed {
            write_release_manifests(root.path(), "0.2.0", "0.2.0", "0.2.0");
            write_file(root.path().join(name), contents);
            let error = release_source(root.path(), "workflow_dispatch", "main")
                .expect_err("malformed release metadata must fail closed");
            assert!(error.contains("malformed"), "unexpected error: {error}");
            assert!(!error.contains(root.path().to_string_lossy().as_ref()));
        }
    }

    #[test]
    fn release_source_push_requires_exact_tag_at_current_origin_main() {
        let root = TempRoot::new("release-source-push");
        write_release_manifests(root.path(), "0.2.0", "0.2.0", "0.2.0");
        initialize_release_git(root.path());

        let source = release_source(root.path(), "push", "v0.2.0")
            .expect("matching stable tag at origin/main");
        assert_eq!(source.channel, ReleaseChannel::Stable);
        assert_eq!(source.version, "0.2.0");
        assert_eq!(
            release_source(root.path(), "push", "v0.2.1"),
            Err("release tag does not match the manifest version".to_string())
        );

        write_file(root.path().join("post-main-change"), b"new commit\n");
        run_git(root.path(), &["add", "post-main-change"]);
        run_git(
            root.path(),
            &[
                "-c",
                "user.name=RepoGrammar Test",
                "-c",
                "user.email=repo-guard@example.invalid",
                "commit",
                "-q",
                "-m",
                "test: advance head",
            ],
        );
        assert_eq!(
            release_source(root.path(), "push", "v0.2.0"),
            Err("release source is not the current origin/main commit".to_string())
        );
    }

    #[test]
    fn stable_dist_tags_require_exact_latest_and_a_preserved_preview() {
        let stable_tags = format!(
            r#"{{"latest":"{STABLE_RELEASE_VERSION}","preview":"{STABLE_PREVIEW_VERSION}"}}"#
        );
        let published_stable =
            format!(r#"["{STABLE_PREVIEW_VERSION}","{STABLE_RELEASE_VERSION}"]"#);
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                STABLE_PREVIEW_VERSION,
                STABLE_RELEASE_VERSION,
                &stable_tags,
                &published_stable,
            ),
            Ok(ReleaseDistTagAction::StableVerified)
        );
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                STABLE_PREVIEW_VERSION,
                "0.1.0",
                r#"{"latest":"0.1.0","preview":"0.2.0-preview.0"}"#,
                r#"["0.1.0","0.2.0-preview.0","0.2.2"]"#,
            ),
            Err("latest does not match the stable manifest version")
        );
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                STABLE_RELEASE_VERSION,
                STABLE_RELEASE_VERSION,
                r#"{"latest":"0.2.2","preview":"0.2.2"}"#,
                r#"["0.2.2"]"#,
            ),
            Err("preview does not match the required stable predecessor")
        );
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                "0.1.0-preview.9",
                STABLE_RELEASE_VERSION,
                r#"{"latest":"0.2.2","preview":"0.1.0-preview.9"}"#,
                r#"["0.1.0-preview.9","0.2.2"]"#,
            ),
            Err("preview does not match the required stable predecessor")
        );
        assert_eq!(
            release_dist_tag_action(
                "0.3.0",
                "0.2.0-preview.0",
                "0.3.0",
                r#"{"latest":"0.3.0","preview":"0.2.0-preview.0"}"#,
                r#"["0.2.0-preview.0","0.3.0"]"#,
            ),
            Err("stable release policy is not registered for this version")
        );
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                STABLE_PREVIEW_VERSION,
                STABLE_RELEASE_VERSION,
                &stable_tags,
                r#"["0.2.2"]"#,
            ),
            Err("preview does not reference a published version")
        );
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                STABLE_PREVIEW_VERSION,
                STABLE_RELEASE_VERSION,
                &stable_tags,
                r#"["0.2.0-preview.0"]"#,
            ),
            Err("stable manifest version is not published")
        );

        for tags in [
            r#"{"latest":"0.2.2"}"#,
            r#"{"latest":"0.2.2","preview":"0.2.0-preview.0","beta":"0.2.2"}"#,
        ] {
            assert_eq!(
                release_dist_tag_action(
                    STABLE_RELEASE_VERSION,
                    STABLE_PREVIEW_VERSION,
                    STABLE_RELEASE_VERSION,
                    tags,
                    &published_stable,
                ),
                Err("dist-tag inventory is not exact")
            );
        }
        assert_eq!(
            release_dist_tag_action(
                STABLE_RELEASE_VERSION,
                STABLE_PREVIEW_VERSION,
                STABLE_RELEASE_VERSION,
                r#"{"latest":"0.2.2","preview":"0.1.0-preview.9"}"#,
                &published_stable,
            ),
            Err("dist-tag inventory does not match the classified values")
        );
        for failed_version in FAILED_STABLE_RELEASE_VERSIONS {
            let published_with_failed = format!(
                r#"["{STABLE_PREVIEW_VERSION}","{failed_version}","{STABLE_RELEASE_VERSION}"]"#
            );
            assert_eq!(
                release_dist_tag_action(
                    STABLE_RELEASE_VERSION,
                    STABLE_PREVIEW_VERSION,
                    STABLE_RELEASE_VERSION,
                    &stable_tags,
                    &published_with_failed,
                ),
                Err("failed stable candidate versions must not be published")
            );
        }
    }

    #[test]
    fn preview_dist_tag_inventory_is_exact_with_or_without_latest() {
        assert_eq!(
            release_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "",
                r#"{"preview":"0.2.0-preview.0"}"#,
                r#"["0.2.0-preview.0"]"#,
            ),
            Ok(ReleaseDistTagAction::Preview(PreviewDistTagAction::NoTag))
        );
        assert_eq!(
            release_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "0.1.0",
                r#"{"latest":"0.1.0","preview":"0.2.0-preview.0"}"#,
                r#"["0.1.0","0.2.0-preview.0"]"#,
            ),
            Ok(ReleaseDistTagAction::Preview(
                PreviewDistTagAction::PreserveStable
            ))
        );
    }

    #[test]
    fn release_channel_and_dist_tag_commands_are_bounded() {
        let channel = run(["release-channel", "--version", "0.2.0"], Path::new("."));
        assert_eq!(channel.status, 0);
        assert_eq!(channel.stdout, "stable\n");

        let final_state = run(
            [
                "release-dist-tag-action",
                "--version",
                "0.2.2",
                "--preview",
                "0.2.0-preview.0",
                "--latest",
                "0.2.2",
                "--tags-json",
                r#"{"latest":"0.2.2","preview":"0.2.0-preview.0"}"#,
                "--versions-json",
                r#"["0.2.0-preview.0","0.2.2"]"#,
            ],
            Path::new("."),
        );
        assert_eq!(final_state.status, 0);
        assert_eq!(final_state.stdout, "stable_latest_verified\n");
    }

    #[test]
    fn preview_dist_tags_without_latest_need_no_write() {
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "",
                r#"["0.2.0-preview.0"]"#,
            ),
            Ok(PreviewDistTagAction::NoTag)
        );
    }

    #[test]
    fn prerelease_latest_is_allowed_only_without_published_stable_versions() {
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                r#"["0.2.0-preview.0"]"#,
            ),
            Ok(PreviewDistTagAction::AllowPrereleaseWithoutStable)
        );
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "0.1.0-preview.9",
                r#"["0.1.0-preview.9","0.2.0-preview.0"]"#,
            ),
            Ok(PreviewDistTagAction::AllowPrereleaseWithoutStable)
        );
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                r#"["0.1.0","0.2.0-preview.0"]"#,
            ),
            Err("latest is a prerelease while stable versions exist")
        );
    }

    #[test]
    fn stable_latest_is_preserved() {
        for latest in ["0.1.0", "0.1.0+build-x"] {
            assert_eq!(
                preview_dist_tag_action(
                    "0.2.0-preview.0",
                    "0.2.0-preview.0",
                    latest,
                    &format!(r#"["{latest}","0.2.0-preview.0"]"#),
                ),
                Ok(PreviewDistTagAction::PreserveStable)
            );
        }
    }

    #[test]
    fn preview_dist_tag_classification_fails_closed() {
        assert_eq!(
            preview_dist_tag_action("0.2.0-preview.0", "", "", r#"["0.2.0-preview.0"]"#,),
            Err("preview does not match the manifest version")
        );
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "banana",
                r#"["0.2.0-preview.0"]"#,
            ),
            Err("latest is malformed")
        );
        assert_eq!(
            preview_dist_tag_action("0.2.0", "0.2.0", "", r#"["0.2.0"]"#),
            Err("manifest version is not a bounded prerelease")
        );
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "0.1.0-preview.9",
                r#"["0.2.0-preview.0"]"#,
            ),
            Err("latest does not reference a published version")
        );
        assert_eq!(
            preview_dist_tag_action(
                "0.2.0-preview.0",
                "0.2.0-preview.0",
                "",
                r#"["0.1.0-preview.9"]"#,
            ),
            Err("manifest version is not published")
        );
        assert_eq!(
            preview_dist_tag_action("0.2.0-preview.0", "0.2.0-preview.0", "", "{}"),
            Err("published versions are malformed")
        );
        let too_many = serde_json::to_string(&vec!["0.2.0-preview.0"; 257])
            .expect("serialize published versions");
        assert_eq!(
            preview_dist_tag_action("0.2.0-preview.0", "0.2.0-preview.0", "", &too_many),
            Err("published version count is outside the supported bound")
        );
    }

    #[test]
    fn preview_dist_tag_command_requires_the_published_version_inventory() {
        let result = run(
            [
                "preview-dist-tag-action",
                "--version",
                "0.2.0-preview.0",
                "--preview",
                "0.2.0-preview.0",
                "--latest",
                "0.2.0-preview.0",
                "--versions-json",
                r#"["0.2.0-preview.0"]"#,
            ],
            Path::new("."),
        );
        assert_eq!(result.status, 0);
        assert_eq!(result.stdout, "allow_prerelease_latest_without_stable\n");

        let missing_inventory = run(
            [
                "preview-dist-tag-action",
                "--version",
                "0.2.0-preview.0",
                "--preview",
                "0.2.0-preview.0",
                "--latest",
                "0.2.0-preview.0",
            ],
            Path::new("."),
        );
        assert_eq!(missing_inventory.status, 1);
        assert!(missing_inventory
            .stderr
            .contains("unknown or invalid arguments"));
    }

    #[test]
    fn release_workflow_requires_oidc_staging_for_preview() {
        let root = TempRoot::new("preview-oidc-stage");
        write_valid_release_contract(root.path());

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations.is_empty(), "{violations:?}");

        let direct = valid_release_workflow().replace(
            "npm stage publish \"${package_file}\" --access public --tag preview --provenance",
            "npm publish \"${package_file}\" --access public --tag preview",
        );
        write_file(
            root.path().join(".github/workflows/release.yml"),
            direct.as_bytes(),
        );
        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations
            .iter()
            .any(|violation| violation.rule == "ReleasePublicationAuthority"));
    }

    #[test]
    fn preview_release_stage_requires_an_explicit_relative_tarball_path() {
        let root = TempRoot::new("preview-release-stage-relative-path");
        write_valid_release_contract(root.path());

        let invalid = valid_release_workflow().replace(
            PREVIEW_NPM_STAGE_PACKAGE_ASSIGNMENT,
            r#"package_file="npm-candidate/sioyooo-repogrammar-${{ needs.classify.outputs.version }}.tgz""#,
        );
        assert_ne!(invalid, valid_release_workflow());
        write_file(
            root.path().join(".github/workflows/release.yml"),
            invalid.as_bytes(),
        );

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(
            violations
                .iter()
                .any(|violation| violation.rule == "NpmCandidateReuseContract"),
            "{violations:?}"
        );
    }

    #[test]
    fn stable_release_workflow_only_stages_after_draft_release_assets() {
        let root = TempRoot::new("stable-release-staging");
        write_valid_release_contract(root.path());

        for invalid in [
            valid_release_workflow().replace("draft: true", "draft: false"),
            valid_release_workflow().replace(
                "needs: [classify, package_npm, prepare_github_release]",
                "needs: classify",
            ),
            valid_release_workflow().replace("id-token: write", "id-token: read"),
            valid_release_workflow().replace(
                STABLE_NPM_STAGE_COMMAND,
                "npm stage inspect ./npm-candidate/sioyooo-repogrammar-0.2.2.tgz --access public --tag latest --provenance",
            ),
        ] {
            write_file(
                root.path().join(".github/workflows/release.yml"),
                invalid.as_bytes(),
            );
            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(violations
                .iter()
                .any(|violation| violation.rule == "StableReleaseStagingContract"));
        }

        let direct_stable = format!(
            "{}\nnpm publish --access public --tag latest\n",
            valid_release_workflow()
        );
        write_file(
            root.path().join(".github/workflows/release.yml"),
            direct_stable.as_bytes(),
        );
        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations
            .iter()
            .any(|violation| violation.rule == "ReleasePublicationAuthority"));
    }

    #[test]
    fn stable_release_stage_requires_the_exact_explicit_relative_tarball_path() {
        let root = TempRoot::new("stable-release-stage-relative-path");
        write_valid_release_contract(root.path());

        let invalid = valid_release_workflow().replace(
            STABLE_NPM_STAGE_COMMAND,
            "npm stage publish npm-candidate/sioyooo-repogrammar-0.2.2.tgz --access public --tag latest --provenance",
        );
        assert_ne!(invalid, valid_release_workflow());
        write_file(
            root.path().join(".github/workflows/release.yml"),
            invalid.as_bytes(),
        );

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(
            violations
                .iter()
                .any(|violation| violation.rule == "StableReleaseStagingContract"),
            "{violations:?}"
        );
    }

    #[test]
    fn release_workflow_rejects_paginated_gh_api_slurp_with_jq() {
        let root = TempRoot::new("release-runner-gh-api-flags");
        write_valid_release_contract(root.path());
        let invalid = valid_release_workflow()
            .replace("gh api --paginate \\\n", "gh api --paginate --slurp \\\n");
        assert_ne!(invalid, valid_release_workflow());
        write_file(
            root.path().join(".github/workflows/release.yml"),
            invalid.as_bytes(),
        );

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(
            violations
                .iter()
                .any(|violation| { violation.rule == "ReleaseWorkflowRunnerCompatibility" }),
            "{violations:?}"
        );
    }

    #[test]
    fn release_workflow_requires_the_exact_draft_collision_guard() {
        let root = TempRoot::new("release-draft-collision-guard");
        write_valid_release_contract(root.path());

        for invalid in [
            valid_release_workflow().replace("gh api --paginate \\\n", "gh api \\\n"),
            valid_release_workflow().replace(
                "repos/${GITHUB_REPOSITORY}/releases?per_page=100",
                "repos/${GITHUB_REPOSITORY}/tags?per_page=100",
            ),
            valid_release_workflow().replace(
                ".[] | select(.tag_name == env.TAG_NAME) | .id",
                ".[][] | select(.tag_name == env.TAG_NAME) | .id",
            ),
            valid_release_workflow().replace(
                "if [ -n \"${existing_release_id}\" ]; then",
                "if [ -z \"${existing_release_id}\" ]; then",
            ),
        ] {
            assert_ne!(invalid, valid_release_workflow());
            write_file(
                root.path().join(".github/workflows/release.yml"),
                invalid.as_bytes(),
            );

            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(
                violations
                    .iter()
                    .any(|violation| { violation.rule == "StableReleaseDraftCollisionContract" }),
                "{violations:?}"
            );
        }
    }

    #[test]
    fn release_workflow_guards_source_outputs_and_immutable_asset_inventory() {
        let root = TempRoot::new("release-source-and-assets");
        write_valid_release_contract(root.path());

        for marker in [
            "release-source --event-name \"${EVENT_NAME}\" --ref-name \"${PUSH_REF_NAME}\"",
            "channel: ${{ steps.release.outputs.channel }}",
            "git fetch --no-tags origin main:refs/remotes/origin/main",
        ] {
            let invalid = valid_release_workflow().replace(marker, "removed");
            write_file(
                root.path().join(".github/workflows/release.yml"),
                invalid.as_bytes(),
            );
            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.rule == "ReleaseSourceContract"),
                "{marker}: {violations:?}"
            );
        }

        for marker in [
            "npm-candidate-manifest.json",
            "= \"11\"",
            "Refuse an existing release or draft for this tag",
            "overwrite_files: false",
            "fail_on_unmatched_files: true",
        ] {
            let invalid = valid_release_workflow().replace(marker, "removed");
            write_file(
                root.path().join(".github/workflows/release.yml"),
                invalid.as_bytes(),
            );
            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.rule == "StableReleaseAssetContract"),
                "{marker}: {violations:?}"
            );
        }
    }

    #[test]
    fn stable_publication_authority_rejects_dynamic_or_comment_only_bypasses() {
        let root = TempRoot::new("stable-release-authority-bypass");
        write_valid_release_contract(root.path());

        let exact = STABLE_NPM_STAGE_COMMAND;
        let dynamic = valid_release_workflow().replace(
            exact,
            &format!("# {exact}\n    npm \"${{subcommand}}\" ./npm-candidate/sioyooo-repogrammar-0.2.2.tgz --access public --tag latest --provenance"),
        );
        write_file(
            root.path().join(".github/workflows/release.yml"),
            dynamic.as_bytes(),
        );
        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations
            .iter()
            .any(|violation| violation.rule == "StableReleaseStagingContract"));

        let repacked = valid_release_workflow().replace(
            "verify-npm-pack-evidence",
            "npm pack --pack-destination npm-candidate",
        );
        write_file(
            root.path().join(".github/workflows/release.yml"),
            repacked.as_bytes(),
        );
        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations
            .iter()
            .any(|violation| violation.rule == "StableReleaseStagingContract"));
    }

    #[test]
    fn manual_release_finalization_cannot_publish_or_mutate_tags() {
        let root = TempRoot::new("manual-tag-verification-mutation");
        write_valid_release_contract(root.path());
        for mutation in [
            "npm publish",
            "npm stage publish",
            "npm stage approve",
            "npm stage reject",
            "npm dist-tag rm",
            "npm dist-tag add",
        ] {
            let invalid = format!("{}\n{mutation}\n", valid_finalize_workflow());
            write_file(
                root.path().join(".github/workflows/npm-tag-reconcile.yml"),
                invalid.as_bytes(),
            );

            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(violations
                .iter()
                .any(|violation| violation.rule == "NpmReleaseFinalizeContract"));
        }
    }

    #[test]
    fn stable_release_finalizer_is_read_only_and_evidence_complete() {
        let root = TempRoot::new("stable-finalizer-authority");
        write_valid_release_contract(root.path());

        for mutation in ["npm publish", "npm stage approve", "id-token: write"] {
            let invalid = format!("{}\n{mutation}\n", valid_stable_finalize_workflow());
            write_file(
                root.path()
                    .join(".github/workflows/stable-release-finalize.yml"),
                invalid.as_bytes(),
            );
            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(violations
                .iter()
                .any(|violation| violation.rule == "StableReleaseFinalizeContract"));
        }

        for marker in [
            "candidate_run_attempt:",
            "--jq '{run_id: .id, run_attempt: .run_attempt, name: .name, path: .path, event: .event, head_branch: .head_branch, head_sha: .head_sha, conclusion: .conclusion}'",
            "keys == [\"conclusion\", \"event\", \"head_branch\", \"head_sha\", \"name\", \"path\", \"run_attempt\", \"run_id\"]",
            "npm-candidate-manifest.json",
            "= \"11\"",
            "verify-npm-pack-evidence",
            "evidence/public-native-smoke.txt",
            "evidence/public-installer-version.txt",
            "evidence/pinned-setup.json",
            "evidence/latest-setup.json",
        ] {
            let invalid = valid_stable_finalize_workflow().replace(marker, "removed");
            write_file(
                root.path()
                    .join(".github/workflows/stable-release-finalize.yml"),
                invalid.as_bytes(),
            );
            let mut violations = Vec::new();
            check_release_workflow_contract(root.path(), &mut violations);
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.rule == "StableReleaseFinalizeContract"),
                "{marker}: {violations:?}"
            );
        }
    }

    fn valid_release_workflow() -> String {
        r#"workflow_dispatch:
  default: build-only
if: github.event_name == 'push' && github.ref_type == 'tag'
draft: true
node-version: 24
npm@11.18.0
  classify:
    outputs:
      channel: ${{ steps.release.outputs.channel }}
      version: ${{ steps.release.outputs.version }}
    steps:
      fetch-depth: 0
      git fetch --no-tags origin main:refs/remotes/origin/main
      id: release
      release-source --event-name "${EVENT_NAME}" --ref-name "${PUSH_REF_NAME}"
  package_npm:
    needs: [classify, verify]
    npm pack --json --ignore-scripts --pack-destination npm-candidate
    smoke-npm-package
    candidate-manifest.json
    verify-npm-pack-evidence
    actions/upload-artifact@v7
    name: npm-package-${{ needs.classify.outputs.version }}
  package_installer:
    name: repogrammar-installer
  prepare_github_release:
    needs: [classify, build, package_installer, package_npm]
    npm-candidate-manifest.json
    test "$(find release-assets -maxdepth 1 -type f | wc -l | tr -d ' ')" = "11"
    Refuse an existing release or draft for this tag
    existing_release_id="$(gh api --paginate \
      "repos/${GITHUB_REPOSITORY}/releases?per_page=100" \
      --jq '.[] | select(.tag_name == env.TAG_NAME) | .id')"
    if [ -n "${existing_release_id}" ]; then
      exit 1
    fi
    overwrite_files: false
    fail_on_unmatched_files: true
  stage_npm_preview:
    environment: npm-release
    id-token: write
    actions/download-artifact@v8
    package_file="./npm-candidate/sioyooo-repogrammar-${{ needs.classify.outputs.version }}.tgz"
    npm stage publish "${package_file}" --access public --tag preview --provenance
  stage_npm_stable:
    needs: [classify, package_npm, prepare_github_release]
    if: github.event_name == 'push' && github.ref_type == 'tag' && needs.classify.outputs.channel == 'stable'
    environment: npm-release
    id-token: write
    node-version: 24
    npm@11.18.0
    actions/download-artifact@v8
    name: npm-package-${{ needs.classify.outputs.version }}
    smoke-npm-package
    verify-npm-pack-evidence
    npm stage publish ./npm-candidate/sioyooo-repogrammar-0.2.2.tgz --access public --tag latest --provenance
"#
        .to_string()
    }

    fn valid_finalize_workflow() -> String {
        r#"workflow_call:
workflow_dispatch:
release-channel
release-dist-tag-action
versions --json
--versions-json
final_action=
node-version: 24
npm@11.18.0
"#
        .to_string()
    }

    fn valid_stable_finalize_workflow() -> String {
        r#"workflow_dispatch:
candidate_run_id:
candidate_run_attempt:
permissions:
contents: read
actions: read
gh release verify v0.2.2
gh release verify-asset v0.2.2
npm-candidate-manifest.json
test "$(find evidence/github-assets -maxdepth 1 -type f | wc -l | tr -d ' ')" = "11"
--jq '{run_id: .id, run_attempt: .run_attempt, name: .name, path: .path, event: .event, head_branch: .head_branch, head_sha: .head_sha, conclusion: .conclusion}'
keys == ["conclusion", "event", "head_branch", "head_sha", "name", "path", "run_attempt", "run_id"]
evidence/candidate-run.json
evidence/public-npm-pack.json
verify-npm-pack-evidence
Smoke the verified public native archive
npm audit signatures --json --include-attestations
--candidate-manifest evidence/retained-candidate-manifest.json
smoke-npm-package
evidence/public-native-smoke.txt
evidence/public-installer-version.txt
evidence/pinned-setup.json
evidence/latest-setup.json
verify-stable-release-evidence --evidence-dir evidence
"#
        .to_string()
    }

    fn write_valid_release_contract(root: &Path) {
        write_file(
            root.join(".github/workflows/release.yml"),
            valid_release_workflow().as_bytes(),
        );
        write_file(
            root.join(".github/workflows/npm-tag-reconcile.yml"),
            valid_finalize_workflow().as_bytes(),
        );
        write_file(
            root.join(".github/workflows/stable-release-finalize.yml"),
            valid_stable_finalize_workflow().as_bytes(),
        );
    }

    #[test]
    fn packaged_artifact_smoke_is_a_documented_command() {
        assert!(usage().contains(
            "smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version>"
        ));
        assert!(usage().contains("smoke-npm-package --tarball <path> --expected-version <version>"));
    }

    #[test]
    fn npm_integrity_base64_is_standard() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        for value in [b"f".as_slice(), b"fo", b"foo", b"foobar"] {
            assert_eq!(
                base64_decode_bounded(&base64_encode(value)),
                Ok(value.to_vec())
            );
        }
        for malformed in ["", "Zg=", "Zh==", "Zm9=", "Zm=v", "Zm9v\n"] {
            assert!(base64_decode_bounded(malformed).is_err(), "{malformed}");
        }
    }

    #[test]
    fn npm_package_smoke_rejects_missing_candidates() {
        let root = TempRoot::new("npm-smoke-missing-input");
        let error = smoke_npm_package(root.path(), "missing.tgz", "0.2.0")
            .expect_err("missing npm candidate must fail closed");
        assert_eq!(error, "npm package candidate is unavailable");
    }

    #[test]
    fn npm_pack_evidence_requires_exact_integrity_and_files() {
        let root = TempRoot::new("npm-pack-evidence");
        let pack = serde_json::json!([{
            "filename": "sioyooo-repogrammar-0.2.0.tgz",
            "integrity": "sha512-example",
            "files": [
                {"path": "LICENSE"},
                {"path": "README.md"},
                {"path": "package.json"},
                {"path": "src/npm/repogrammar.js"}
            ]
        }]);
        let manifest = serde_json::json!({
            "schema_version": 1,
            "package_name": NPM_PACKAGE_NAME,
            "version": "0.2.0",
            "filename": "sioyooo-repogrammar-0.2.0.tgz",
            "integrity": "sha512-example",
            "files": NPM_PACKAGE_FILES,
            "offline_install_smoke": "passed",
            "local_release_asset_smoke": "passed"
        });
        write_file(
            root.path().join("pack.json"),
            serde_json::to_vec(&pack).expect("pack JSON").as_slice(),
        );
        write_file(
            root.path().join("manifest.json"),
            serde_json::to_vec(&manifest)
                .expect("manifest JSON")
                .as_slice(),
        );
        assert!(
            verify_npm_pack_evidence(root.path(), "pack.json", "manifest.json", "0.2.0").is_ok()
        );

        let mut mismatched = manifest;
        mismatched["integrity"] = serde_json::Value::String("sha512-other".to_string());
        write_file(
            root.path().join("manifest.json"),
            serde_json::to_vec(&mismatched)
                .expect("mismatched manifest JSON")
                .as_slice(),
        );
        assert_eq!(
            verify_npm_pack_evidence(root.path(), "pack.json", "manifest.json", "0.2.0"),
            Err("npm pack metadata and candidate evidence do not agree".to_string())
        );

        let mut unexpected = stable_candidate_manifest();
        unexpected["unexpected"] = serde_json::Value::Bool(true);
        assert_eq!(
            validate_npm_candidate_manifest(&unexpected, STABLE_RELEASE_VERSION),
            Err("npm candidate evidence manifest is incomplete".to_string())
        );
    }

    #[test]
    fn stable_release_evidence_fails_closed_when_incomplete() {
        let root = TempRoot::new("stable-release-evidence-incomplete");
        fs::create_dir_all(root.path().join("evidence")).expect("create evidence directory");
        let error = verify_stable_release_evidence(root.path(), "evidence")
            .expect_err("incomplete release evidence must fail closed");
        assert_eq!(error, "required release evidence is unavailable");
    }

    #[test]
    fn stable_release_evidence_accepts_complete_public_evidence() {
        let root = TempRoot::new("stable-release-evidence-complete");
        write_stable_evidence_fixture(root.path());

        assert_eq!(STABLE_RELEASE_ASSETS.len(), 11);
        assert_eq!(
            verify_stable_release_evidence(root.path(), "evidence"),
            Ok(())
        );
        let command = run(
            [
                "verify-stable-release-evidence",
                "--evidence-dir",
                "evidence",
            ],
            root.path(),
        );
        assert_eq!(command.status, 0);
        assert_eq!(command.stdout, "STABLE_RELEASE_READY\n");
        assert!(command.stderr.is_empty());
    }

    #[test]
    fn npm_11_18_live_audit_shape_selects_the_single_slsa_bundle() {
        let statement = stable_provenance_statement();
        let audit = stable_audit_signatures(&statement);
        assert!(audit["verified"][0]["attestations"].is_object());
        assert_eq!(
            audit["verified"][0]["attestationBundles"]
                .as_array()
                .expect("npm 11.18 attestation bundles")
                .len(),
            2
        );
        assert_eq!(
            verify_npm_provenance(
                &audit,
                stable_candidate_manifest()["sha512"]
                    .as_str()
                    .expect("candidate digest"),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                123,
                2,
            ),
            Ok(())
        );
    }

    #[test]
    fn stable_release_provenance_rejects_every_bound_identity_mutation() {
        for field in [
            "digest",
            "repository",
            "workflow_path",
            "ref",
            "event",
            "dependency_uri",
            "head_sha",
            "run_id",
            "run_attempt",
            "predicate",
            "builder",
        ] {
            let root = TempRoot::new(&format!("stable-provenance-{field}"));
            write_stable_evidence_fixture(root.path());
            mutate_provenance_statement(root.path(), |statement| match field {
                "digest" => {
                    statement["subject"][0]["digest"]["sha512"] =
                        serde_json::Value::String("00".repeat(64));
                }
                "repository" => {
                    statement["predicate"]["buildDefinition"]["externalParameters"]["workflow"]
                        ["repository"] = serde_json::Value::String(
                        "https://github.com/foreign/RepoGrammar".to_string(),
                    );
                }
                "workflow_path" => {
                    statement["predicate"]["buildDefinition"]["externalParameters"]["workflow"]
                        ["path"] =
                        serde_json::Value::String("/.github/workflows/release.yml".to_string());
                }
                "ref" => {
                    statement["predicate"]["buildDefinition"]["externalParameters"]["workflow"]
                        ["ref"] = serde_json::Value::String("refs/heads/main".to_string());
                }
                "event" => {
                    statement["predicate"]["buildDefinition"]["internalParameters"]["github"]
                        ["event_name"] = serde_json::Value::String("workflow_dispatch".to_string());
                }
                "dependency_uri" => {
                    statement["predicate"]["buildDefinition"]["resolvedDependencies"][0]["uri"] =
                        serde_json::Value::String(
                            "git+https://github.com/foreign/RepoGrammar@refs/tags/v0.2.2"
                                .to_string(),
                        );
                }
                "head_sha" => {
                    statement["predicate"]["buildDefinition"]["resolvedDependencies"][0]
                        ["digest"]["gitCommit"] = serde_json::Value::String("b".repeat(40));
                }
                "run_id" => {
                    statement["predicate"]["runDetails"]["metadata"]["invocationId"] =
                        serde_json::Value::String(
                            "https://github.com/SioYooo/RepoGrammar/actions/runs/124/attempts/2"
                                .to_string(),
                        );
                }
                "run_attempt" => {
                    statement["predicate"]["runDetails"]["metadata"]["invocationId"] =
                        serde_json::Value::String(
                            "https://github.com/SioYooo/RepoGrammar/actions/runs/123/attempts/3"
                                .to_string(),
                        );
                }
                "predicate" => {
                    statement["predicateType"] =
                        serde_json::Value::String("https://example.invalid/predicate".to_string());
                }
                "builder" => {
                    statement["predicate"]["runDetails"]["builder"]["id"] =
                        serde_json::Value::String("https://example.invalid/builder".to_string());
                }
                _ => unreachable!(),
            });

            let error = verify_stable_release_evidence(root.path(), "evidence")
                .expect_err("mutated provenance must fail closed");
            assert!(error.contains("provenance"), "{field}: {error}");
        }
    }

    #[test]
    fn stable_release_provenance_rejects_package_and_audit_mutations() {
        for field in [
            "package",
            "version",
            "predicate_inventory",
            "legacy_attestation_array",
            "missing",
            "invalid",
            "verified_count",
            "payload",
            "payload_type",
            "missing_publish_bundle",
            "duplicate_publish_bundle",
            "extra_third_bundle",
            "duplicate_slsa_bundle",
        ] {
            let root = TempRoot::new(&format!("stable-audit-{field}"));
            write_stable_evidence_fixture(root.path());
            let path = root.path().join("evidence/npm-audit-signatures.json");
            let mut audit = read_json(&path);
            match field {
                "package" => {
                    audit["verified"][0]["name"] =
                        serde_json::Value::String("@foreign/repogrammar".to_string());
                }
                "version" => {
                    audit["verified"][0]["version"] =
                        serde_json::Value::String("0.2.0".to_string());
                }
                "predicate_inventory" => {
                    audit["verified"][0]["attestations"]["provenance"]["predicateType"] =
                        serde_json::Value::String("https://example.invalid/predicate".to_string());
                }
                "legacy_attestation_array" => {
                    audit["verified"][0]["attestations"] =
                        serde_json::json!([{"predicateType": SLSA_PROVENANCE_V1}]);
                }
                "missing" => audit["missing"] = serde_json::json!([{"name": NPM_PACKAGE_NAME}]),
                "invalid" => audit["invalid"] = serde_json::json!([{"name": NPM_PACKAGE_NAME}]),
                "verified_count" => {
                    let duplicate = audit["verified"][0].clone();
                    audit["verified"]
                        .as_array_mut()
                        .expect("verified array")
                        .push(duplicate);
                }
                "payload" => {
                    audit["verified"][0]["attestationBundles"][1]["bundle"]["dsseEnvelope"]
                        ["payload"] = serde_json::Value::String("not base64".to_string());
                }
                "payload_type" => {
                    audit["verified"][0]["attestationBundles"][1]["bundle"]["dsseEnvelope"]
                        ["payloadType"] = serde_json::Value::String("application/json".to_string());
                }
                "missing_publish_bundle" => {
                    audit["verified"][0]["attestationBundles"]
                        .as_array_mut()
                        .expect("attestation bundle array")
                        .remove(0);
                }
                "duplicate_publish_bundle" => {
                    let duplicate = audit["verified"][0]["attestationBundles"][0].clone();
                    audit["verified"][0]["attestationBundles"][1] = duplicate;
                }
                "extra_third_bundle" => {
                    audit["verified"][0]["attestationBundles"]
                        .as_array_mut()
                        .expect("attestation bundle array")
                        .push(serde_json::json!({
                            "predicateType": "https://example.invalid/extra",
                            "bundle": {}
                        }));
                }
                "duplicate_slsa_bundle" => {
                    let duplicate = audit["verified"][0]["attestationBundles"][1].clone();
                    audit["verified"][0]["attestationBundles"]
                        .as_array_mut()
                        .expect("attestation bundle array")
                        .push(duplicate);
                }
                _ => unreachable!(),
            }
            write_json(path, &audit);

            let error = verify_stable_release_evidence(root.path(), "evidence")
                .expect_err("mutated npm audit evidence must fail closed");
            assert!(
                error.contains("npm")
                    || error.contains("provenance")
                    || error.contains("signature"),
                "{field}: {error}"
            );
        }
    }

    #[test]
    fn stable_release_evidence_requires_exact_assets_and_candidate_identity() {
        for field in [
            "missing_asset",
            "extra_asset",
            "asset_digest",
            "candidate_copy",
        ] {
            let root = TempRoot::new(&format!("stable-assets-{field}"));
            write_stable_evidence_fixture(root.path());
            let evidence = root.path().join("evidence");
            match field {
                "missing_asset" => fs::remove_file(evidence.join("github-assets/install.sh"))
                    .expect("remove synthetic asset"),
                "extra_asset" => write_file(
                    evidence.join("github-assets/unsupported.zip"),
                    b"unsupported\n",
                ),
                "asset_digest" => write_file(
                    evidence.join("github-assets/install.sh"),
                    b"corrupted installer\n",
                ),
                "candidate_copy" => {
                    let path = evidence.join("public-candidate-manifest.json");
                    let mut manifest = read_json(&path);
                    manifest["version"] = serde_json::Value::String("0.2.0".to_string());
                    write_json(path, &manifest);
                }
                _ => unreachable!(),
            }

            assert!(verify_stable_release_evidence(root.path(), "evidence").is_err());
        }

        let root = TempRoot::new("stable-assets-duplicate-record");
        write_stable_evidence_fixture(root.path());
        let path = root.path().join("evidence/github-release.json");
        let mut release = read_json(&path);
        let duplicate = release["assets"][0].clone();
        release["assets"]
            .as_array_mut()
            .expect("release assets")
            .push(duplicate);
        write_json(path, &release);
        assert_eq!(
            verify_stable_release_evidence(root.path(), "evidence"),
            Err("GitHub release asset inventory is not exact".to_string())
        );
    }

    #[test]
    fn stable_release_evidence_requires_exact_public_smoke_truth() {
        for field in [
            "tag_key",
            "native_smoke",
            "installer_version",
            "pinned_setup",
            "latest_setup",
            "dry_run_setup",
        ] {
            let root = TempRoot::new(&format!("stable-public-smoke-{field}"));
            write_stable_evidence_fixture(root.path());
            let evidence = root.path().join("evidence");
            match field {
                "tag_key" => {
                    let path = evidence.join("npm-tags.json");
                    let mut tags = read_json(&path);
                    tags["legacy"] = serde_json::Value::String("0.1.0".to_string());
                    write_json(path, &tags);
                }
                "native_smoke" => write_file(
                    evidence.join("public-native-smoke.txt"),
                    b"source-tree smoke passed\n",
                ),
                "installer_version" => write_file(
                    evidence.join("public-installer-version.txt"),
                    b"repogrammar 0.2.0-preview.0\n",
                ),
                "pinned_setup" | "latest_setup" => {
                    let path = evidence.join(format!(
                        "{}-setup.json",
                        field.strip_suffix("_setup").expect("setup field prefix")
                    ));
                    let mut setup = read_json(&path);
                    setup["agent_query_ready"] = serde_json::Value::Bool(true);
                    write_json(path, &setup);
                }
                "dry_run_setup" => {
                    let path = evidence.join("setup.json");
                    let mut setup = read_json(&path);
                    setup["repository_index_ready"] = serde_json::Value::Bool(true);
                    write_json(path, &setup);
                }
                _ => unreachable!(),
            }

            assert!(verify_stable_release_evidence(root.path(), "evidence").is_err());
        }

        let root = TempRoot::new("stable-candidate-run-extra-field");
        write_stable_evidence_fixture(root.path());
        let path = root.path().join("evidence/candidate-run.json");
        let mut run = read_json(&path);
        run["html_url"] = serde_json::Value::String("https://example.invalid/raw".to_string());
        write_json(path, &run);
        assert_eq!(
            verify_stable_release_evidence(root.path(), "evidence"),
            Err("candidate run evidence fields are not exact".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn npm_package_smoke_rejects_symlinked_candidates() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("npm-smoke-symlink");
        write_file(root.path().join("candidate.tgz"), b"not a package\n");
        symlink(
            root.path().join("candidate.tgz"),
            root.path().join("sioyooo-repogrammar-0.2.0.tgz"),
        )
        .expect("create npm candidate symlink");
        let error = smoke_npm_package(root.path(), "sioyooo-repogrammar-0.2.0.tgz", "0.2.0")
            .expect_err("symlinked npm candidate must fail closed");
        assert_eq!(error, "npm package candidate must be a regular file");
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

    fn write_release_manifests(
        root: &Path,
        package_version: &str,
        cargo_version: &str,
        lock_version: &str,
    ) {
        write_file(
            root.join("package.json"),
            serde_json::to_vec(&serde_json::json!({"version": package_version}))
                .expect("serialize package manifest")
                .as_slice(),
        );
        write_file(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"repogrammar\"\nversion = \"{cargo_version}\"\n")
                .as_bytes(),
        );
        write_file(
            root.join("Cargo.lock"),
            format!(
                "version = 4\n\n[[package]]\nname = \"repogrammar\"\nversion = \"{lock_version}\"\n"
            )
            .as_bytes(),
        );
    }

    fn initialize_release_git(root: &Path) {
        run_git(root, &["init", "-q"]);
        run_git(root, &["add", "package.json", "Cargo.toml", "Cargo.lock"]);
        run_git(
            root,
            &[
                "-c",
                "user.name=RepoGrammar Test",
                "-c",
                "user.email=repo-guard@example.invalid",
                "commit",
                "-q",
                "-m",
                "test: release source",
            ],
        );
        run_git(root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    }

    fn stable_candidate_manifest() -> serde_json::Value {
        let digest = [0x2a_u8; 64];
        serde_json::json!({
            "schema_version": 1,
            "package_name": NPM_PACKAGE_NAME,
            "version": STABLE_RELEASE_VERSION,
            "filename": "sioyooo-repogrammar-0.2.2.tgz",
            "sha512": hex_digest(&digest),
            "integrity": format!("sha512-{}", base64_encode(&digest)),
            "files": NPM_PACKAGE_FILES,
            "offline_install_smoke": "passed",
            "local_release_asset_smoke": "passed"
        })
    }

    fn stable_provenance_statement() -> serde_json::Value {
        serde_json::json!({
            "_type": IN_TOTO_STATEMENT_V1,
            "subject": [{
                "name": "pkg:npm/%40sioyooo/repogrammar@0.2.2",
                "digest": {"sha512": hex_digest(&[0x2a_u8; 64])}
            }],
            "predicateType": SLSA_PROVENANCE_V1,
            "predicate": {
                "buildDefinition": {
                    "buildType": GITHUB_WORKFLOW_BUILD_TYPE_V1,
                    "externalParameters": {
                        "workflow": {
                            "repository": RELEASE_REPOSITORY_URL,
                            "path": RELEASE_WORKFLOW_PATH,
                            "ref": "refs/tags/v0.2.2"
                        }
                    },
                    "internalParameters": {"github": {"event_name": "push"}},
                    "resolvedDependencies": [{
                        "uri": RELEASE_DEPENDENCY_URI,
                        "digest": {"gitCommit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
                    }]
                },
                "runDetails": {
                    "builder": {"id": GITHUB_HOSTED_BUILDER},
                    "metadata": {
                        "invocationId": "https://github.com/SioYooo/RepoGrammar/actions/runs/123/attempts/2"
                    }
                }
            }
        })
    }

    fn stable_audit_signatures(statement: &serde_json::Value) -> serde_json::Value {
        let payload = base64_encode(
            &serde_json::to_vec(statement).expect("serialize synthetic provenance statement"),
        );
        let publish_payload = base64_encode(
            &serde_json::to_vec(&serde_json::json!({
                "_type": IN_TOTO_STATEMENT_V1,
                "predicateType": NPM_PUBLISH_ATTESTATION_V1
            }))
            .expect("serialize synthetic publish statement"),
        );
        serde_json::json!({
            "invalid": [],
            "missing": [],
            "verified": [{
                "name": NPM_PACKAGE_NAME,
                "version": STABLE_RELEASE_VERSION,
                "location": "node_modules/@sioyooo/repogrammar",
                "registry": NPM_REGISTRY_URL,
                "attestations": {
                    "url": "https://registry.npmjs.org/-/npm/v1/attestations/@sioyooo%2frepogrammar@0.2.2",
                    "provenance": {"predicateType": SLSA_PROVENANCE_V1}
                },
                "attestationBundles": [
                    {
                        "predicateType": NPM_PUBLISH_ATTESTATION_V1,
                        "bundle": {"dsseEnvelope": {
                            "payload": publish_payload,
                            "payloadType": IN_TOTO_PAYLOAD_TYPE,
                            "signatures": [{"keyid": "", "sig": "synthetic-publish"}]
                        }}
                    },
                    {
                        "predicateType": SLSA_PROVENANCE_V1,
                        "bundle": {"dsseEnvelope": {
                            "payload": payload,
                            "payloadType": IN_TOTO_PAYLOAD_TYPE,
                            "signatures": [{"keyid": "", "sig": "synthetic-provenance"}]
                        }}
                    }
                ]
            }]
        })
    }

    fn write_stable_evidence_fixture(root: &Path) {
        let evidence = root.join("evidence");
        let assets = evidence.join("github-assets");
        fs::create_dir_all(&assets).expect("create stable evidence asset directory");

        let manifest = stable_candidate_manifest();
        write_json(assets.join("npm-candidate-manifest.json"), &manifest);
        for asset in [
            "install.sh",
            "repogrammar-aarch64-apple-darwin.tar.gz",
            "repogrammar-aarch64-unknown-linux-gnu.tar.gz",
            "repogrammar-x86_64-apple-darwin.tar.gz",
            "repogrammar-x86_64-unknown-linux-gnu.tar.gz",
        ] {
            write_file(
                assets.join(asset),
                format!("synthetic {asset}\n").as_bytes(),
            );
            let bytes = fs::read(assets.join(asset)).expect("read synthetic asset");
            let checksum = format!("{}  {asset}\n", hex_digest(&Sha256::digest(bytes)));
            write_file(assets.join(format!("{asset}.sha256")), checksum.as_bytes());
        }

        let release_assets = STABLE_RELEASE_ASSETS
            .iter()
            .map(|asset| {
                serde_json::json!({
                    "name": asset,
                    "state": "uploaded",
                    "digest": sha256_file_sri(&assets.join(asset)).expect("hash synthetic asset")
                })
            })
            .collect::<Vec<_>>();
        write_json(
            evidence.join("github-release.json"),
            &serde_json::json!({
                "tag_name": "v0.2.2",
                "draft": false,
                "prerelease": false,
                "immutable": true,
                "assets": release_assets
            }),
        );
        write_json(
            evidence.join("github-release-attestation.json"),
            &serde_json::json!({"verified": true}),
        );
        for asset in STABLE_RELEASE_ASSETS {
            write_json(
                evidence.join(format!("asset-attestation-{asset}.json")),
                &serde_json::json!({"verified": true}),
            );
        }

        write_json(evidence.join("retained-candidate-manifest.json"), &manifest);
        write_json(evidence.join("public-candidate-manifest.json"), &manifest);
        write_json(
            evidence.join("public-npm-pack.json"),
            &serde_json::json!([{
                "filename": "sioyooo-repogrammar-0.2.2.tgz",
                "integrity": manifest["integrity"],
                "files": [
                    {"path": "LICENSE"},
                    {"path": "README.md"},
                    {"path": "package.json"},
                    {"path": "src/npm/repogrammar.js"}
                ]
            }]),
        );
        write_file(
            evidence.join("npm-registry-integrity.txt"),
            format!(
                "{}\n",
                manifest["integrity"].as_str().expect("candidate SRI")
            )
            .as_bytes(),
        );
        write_json(
            evidence.join("npm-tags.json"),
            &serde_json::json!({
                "latest": STABLE_RELEASE_VERSION,
                "preview": STABLE_PREVIEW_VERSION
            }),
        );
        write_json(
            evidence.join("npm-versions.json"),
            &serde_json::json!([STABLE_PREVIEW_VERSION, STABLE_RELEASE_VERSION]),
        );
        write_json(
            evidence.join("candidate-run.json"),
            &serde_json::json!({
                "run_id": 123,
                "run_attempt": 2,
                "name": "Release",
                "path": RELEASE_WORKFLOW_PATH,
                "event": "push",
                "head_branch": "v0.2.2",
                "head_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "conclusion": "success"
            }),
        );
        write_file(
            evidence.join("expected-head-sha.txt"),
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        );
        write_json(
            evidence.join("npm-audit-signatures.json"),
            &stable_audit_signatures(&stable_provenance_statement()),
        );
        for (name, value) in [
            ("pinned-version.txt", "repogrammar 0.2.2\n"),
            ("latest-version.txt", "repogrammar 0.2.2\n"),
            ("preview-version.txt", "repogrammar 0.2.0-preview.0\n"),
            ("public-installer-version.txt", "repogrammar 0.2.2\n"),
            (
                "public-native-smoke.txt",
                "packaged artifact smoke passed\n",
            ),
        ] {
            write_file(evidence.join(name), value.as_bytes());
        }
        let live_setup = serde_json::json!({
            "product_self_test_state": "passed",
            "repository_index_ready": true,
            "agent_query_ready": false,
            "suggested_question": null
        });
        write_json(evidence.join("pinned-setup.json"), &live_setup);
        write_json(evidence.join("latest-setup.json"), &live_setup);
        write_json(
            evidence.join("setup.json"),
            &serde_json::json!({
                "status": "dry_run",
                "repository_index_ready": false,
                "agent_query_ready": false,
                "suggested_question": null
            }),
        );
    }

    fn mutate_provenance_statement(root: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
        let path = root.join("evidence/npm-audit-signatures.json");
        let mut audit = read_json(&path);
        let slsa_index = audit["verified"][0]["attestationBundles"]
            .as_array()
            .expect("synthetic attestation bundles")
            .iter()
            .position(|bundle| bundle["predicateType"] == SLSA_PROVENANCE_V1)
            .expect("synthetic SLSA bundle");
        let payload = audit["verified"][0]["attestationBundles"][slsa_index]["bundle"]
            ["dsseEnvelope"]["payload"]
            .as_str()
            .expect("synthetic DSSE payload");
        let mut statement: serde_json::Value = serde_json::from_slice(
            &base64_decode_bounded(payload).expect("decode synthetic DSSE payload"),
        )
        .expect("parse synthetic provenance statement");
        mutate(&mut statement);
        audit["verified"][0]["attestationBundles"][slsa_index]["bundle"]["dsseEnvelope"]
            ["payload"] = serde_json::Value::String(base64_encode(
            &serde_json::to_vec(&statement).expect("serialize mutated provenance statement"),
        ));
        write_json(path, &audit);
    }

    fn read_json(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(path).expect("read JSON fixture"))
            .expect("parse JSON fixture")
    }

    fn write_json(path: PathBuf, value: &serde_json::Value) {
        write_file(
            path,
            serde_json::to_vec_pretty(value)
                .expect("serialize JSON fixture")
                .as_slice(),
        );
    }

    fn run_git(root: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .expect("execute git");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_file(path: PathBuf, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write file");
    }
}
