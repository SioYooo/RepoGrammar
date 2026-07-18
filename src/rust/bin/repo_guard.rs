use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use repogrammar::adapters::persistence::sqlite::SqliteIndexStore;
use repogrammar::application::install::{managed_instruction_block, MANAGED_INSTRUCTION_VERSION};
use repogrammar::ports::index_store::IndexStore;
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
        [command, rest @ ..] if command == "product-eval" => match product_eval_command(root, rest)
        {
            Ok(report) => CommandResult::ok(report),
            Err(error) => CommandResult::err(format!("product evaluation failed: {error}\n")),
        },
        [command, rest @ ..] if command == "sync-equivalence" => {
            match sync_equivalence_command(root, rest) {
                Ok(summary) => CommandResult::ok(summary),
                Err(error) => CommandResult::err(format!("sync-equivalence failed: {error}\n")),
            }
        }
        [command, rest @ ..] if command == "payload-measure" => {
            match payload_measure_command(root, rest) {
                Ok(report) => CommandResult::ok(report),
                Err(error) => CommandResult::err(format!("payload measurement failed: {error}\n")),
            }
        }
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
    "Usage: repo-guard check | sync-agent-guides --from <AGENTS.md|CLAUDE.md> | check-diff --base <rev> --head <rev> | product-eval --corpus <path> --out <dir> [--repetitions <n>] [--bin <path>] [--condition <token>] [--baseline token-overlap] | sync-equivalence --fixture <repo-relative-fixture-root> [--scenario <id> | --all] [--bin <path>] --out <dir> | payload-measure --out <dir> [--bin <path>] [--fixture <repo-relative-fixture-root>] | smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version> | smoke-npm-package --tarball <path> --expected-version <version> | verify-npm-pack-evidence --pack-json <path> --candidate-manifest <path> --expected-version <version> | verify-stable-release-evidence --evidence-dir <path> | release-source --event-name <workflow_dispatch|push> --ref-name <name> | release-channel --version <version> | release-dist-tag-action --version <version> --preview <version-or-empty> --latest <version-or-empty> --tags-json <json-object> --versions-json <json-array> | preview-dist-tag-action --version <version> --preview <version> --latest <version-or-empty> --versions-json <json-array>"
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

const PRODUCT_EVAL_CORPUS_SCHEMA: &str = "product-eval-corpus.v1";
const PRODUCT_EVAL_RESULTS_SCHEMA: &str = "product-eval-results.v2";

/// Default recorded `condition` for a plain product run. `--condition <token>`
/// overrides it verbatim so ablation runs (product built with ablation
/// env/flags) are recorded distinctly under the same results schema.
const PRODUCT_EVAL_CONDITION_DEFAULT: &str = "product";
/// Recorded `condition` for the token-overlap naive baseline control.
const BASELINE_TOKEN_OVERLAP_CONDITION: &str = "baseline_token_overlap";
/// Query tokens shorter than this (in characters) are dropped before scoring.
const BASELINE_MIN_TOKEN_LEN: usize = 3;
/// Minimum token-overlap score required before the baseline may select a family.
const BASELINE_SELECT_MIN_SCORE: usize = 2;
/// Candidate-list depth K at which `candidate_recall` and MRR are evaluated, for
/// every condition. The baseline also caps its own reported ranking at this K, so
/// list-quality metrics compare like for like across product and baseline runs.
const RETRIEVAL_CANDIDATE_K: usize = 5;

/// A naive deterministic retrieval control evaluated on the same corpus gold as
/// the product, emitted in the same results schema. It exists to contrast the
/// product against an honest lower bound; it must never be tuned to flatter or
/// diminish either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalBaseline {
    TokenOverlap,
}

impl EvalBaseline {
    fn from_token(token: &str) -> Result<Self, String> {
        match token {
            "token-overlap" => Ok(EvalBaseline::TokenOverlap),
            other => Err(format!("unsupported --baseline '{other}'")),
        }
    }

    /// Stable token recorded in the results document's `baseline` field.
    fn as_str(self) -> &'static str {
        match self {
            EvalBaseline::TokenOverlap => "token-overlap",
        }
    }

    /// The `condition` recorded when `--condition` is not given explicitly.
    fn default_condition(self) -> &'static str {
        match self {
            EvalBaseline::TokenOverlap => BASELINE_TOKEN_OVERLAP_CONDITION,
        }
    }
}

/// Resolves the recorded `condition` from an optional explicit `--condition` and
/// the selected baseline. An explicit `product` alongside a baseline is rejected
/// (a baseline is not the product); otherwise an explicit token wins verbatim, a
/// baseline contributes its default condition, and a plain run records `product`.
fn resolve_eval_condition(
    explicit: Option<String>,
    baseline: Option<EvalBaseline>,
) -> Result<String, String> {
    if baseline.is_some() && explicit.as_deref() == Some(PRODUCT_EVAL_CONDITION_DEFAULT) {
        return Err(
            "--condition product is incompatible with --baseline; a baseline is not the product"
                .to_string(),
        );
    }
    Ok(explicit.unwrap_or_else(|| {
        baseline
            .map(|baseline| baseline.default_condition().to_string())
            .unwrap_or_else(|| PRODUCT_EVAL_CONDITION_DEFAULT.to_string())
    }))
}

/// Validates a low-cardinality condition token used to tag a results document.
/// Accepts `[a-z0-9_-]+` up to 40 characters and returns it verbatim.
fn validate_condition_token(token: &str) -> Result<String, String> {
    if token.is_empty() {
        return Err("--condition must not be empty".to_string());
    }
    // Reject a leading '-' so a forgotten flag value (e.g. `--condition --baseline`)
    // is a hard error rather than a silently accepted condition token.
    if token.starts_with('-') {
        return Err("--condition must not start with '-'".to_string());
    }
    if !token
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("--condition must match [a-z0-9_-]+".to_string());
    }
    if token.chars().count() > 40 {
        return Err("--condition must be at most 40 characters".to_string());
    }
    Ok(token.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalOutcome {
    Ok,
    PartialContext,
    Unknown,
    Fallback,
}

impl EvalOutcome {
    fn as_str(self) -> &'static str {
        match self {
            EvalOutcome::Ok => "ok",
            EvalOutcome::PartialContext => "partial_context",
            EvalOutcome::Unknown => "unknown",
            EvalOutcome::Fallback => "fallback",
        }
    }

    fn from_token(token: &str) -> Result<Self, String> {
        match token {
            "ok" => Ok(EvalOutcome::Ok),
            "partial_context" => Ok(EvalOutcome::PartialContext),
            "unknown" => Ok(EvalOutcome::Unknown),
            "fallback" => Ok(EvalOutcome::Fallback),
            other => Err(format!("unsupported expected outcome '{other}'")),
        }
    }

    /// Maps a product query `status` string onto the coarse retrieval outcome.
    /// `CONTEXT_ONLY` is the `check` operation's context-success status: a single
    /// family was discovered and hydrated (route `discover_hydrate_compose`), so
    /// it is treated as `ok` on the retrieval axis while conformance stays
    /// advisory. Any unrecognized status is conservatively reported as fallback.
    fn classify_status(status: &str) -> Self {
        match status {
            "ok" | "OK" | "CONTEXT_ONLY" => EvalOutcome::Ok,
            // Static-alignment certificates commit a compared answer, so a
            // committed certificate (aligned or a definite deviation) is `ok`, a
            // partial alignment maps to partial context, and an abstaining
            // certificate maps to unknown. This mirrors
            // `AlignmentStatus::outcome_class`, so a committed retrieval-intent
            // check query keeps its MRR credit instead of being zeroed as a
            // fallback.
            "STATICALLY_ALIGNED" | "STATIC_DEVIATION" => EvalOutcome::Ok,
            "PARTIAL_CONTEXT" | "PARTIAL_ALIGNMENT" => EvalOutcome::PartialContext,
            "UNKNOWN" | "INSUFFICIENT_EVIDENCE" => EvalOutcome::Unknown,
            _ => EvalOutcome::Fallback,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalOperation {
    Find,
    Family,
    Member,
    Explain,
    Check,
}

impl EvalOperation {
    fn as_str(self) -> &'static str {
        match self {
            EvalOperation::Find => "find",
            EvalOperation::Family => "family",
            EvalOperation::Member => "member",
            EvalOperation::Explain => "explain",
            EvalOperation::Check => "check",
        }
    }

    fn from_token(token: &str) -> Result<Self, String> {
        match token {
            "find" => Ok(EvalOperation::Find),
            "family" => Ok(EvalOperation::Family),
            "member" => Ok(EvalOperation::Member),
            "explain" => Ok(EvalOperation::Explain),
            "check" => Ok(EvalOperation::Check),
            other => Err(format!("unsupported query operation '{other}'")),
        }
    }
}

/// Declares what correct behavior a query is measuring. `retrieval` means a
/// specific family should be resolved; `abstention` means the correct behavior
/// is a typed `UNKNOWN` (ambiguous, unsupported, unsafe, or stale input);
/// `context` means metadata-only local context (`PARTIAL_CONTEXT` or a
/// zero-family repository). The intent partitions the retrieval metrics below;
/// it does not change the raw match verdict, which stays field-by-field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalIntent {
    Retrieval,
    Abstention,
    Context,
}

impl EvalIntent {
    fn as_str(self) -> &'static str {
        match self {
            EvalIntent::Retrieval => "retrieval",
            EvalIntent::Abstention => "abstention",
            EvalIntent::Context => "context",
        }
    }

    fn from_token(token: &str) -> Result<Self, String> {
        match token {
            "retrieval" => Ok(EvalIntent::Retrieval),
            "abstention" => Ok(EvalIntent::Abstention),
            "context" => Ok(EvalIntent::Context),
            other => Err(format!("unsupported query intent '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EvalExpected {
    outcome: Option<EvalOutcome>,
    family: Option<String>,
    family_prefix: Option<String>,
    family_any_of: Option<Vec<String>>,
    /// Family-id prefixes that should appear in the actual candidate set. Gold
    /// for Recall@K/MRR; independent of which single family (if any) is
    /// selected. An empty/absent list leaves the query out of candidate recall.
    candidates_include: Option<Vec<String>>,
    unknown_reason: Option<String>,
    route: Option<String>,
    /// First-class matcher for the static-alignment certificate's
    /// `alignment_status` token (e.g. `STATICALLY_ALIGNED`, `STATIC_DEVIATION`,
    /// `PARTIAL_ALIGNMENT`, `INSUFFICIENT_EVIDENCE`). A mismatch is a query
    /// mismatch, so alignment golds are enforced rather than decorative.
    alignment_status: Option<String>,
}

impl EvalExpected {
    fn from_value(value: &serde_json::Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "query expected must be an object".to_string())?;
        let outcome = match object.get("outcome") {
            None | Some(serde_json::Value::Null) => None,
            Some(token) => {
                Some(EvalOutcome::from_token(token.as_str().ok_or_else(
                    || "expected.outcome must be a string".to_string(),
                )?)?)
            }
        };
        let family_any_of = optional_string_array(object, "family_any_of")?;
        let candidates_include = optional_string_array(object, "candidates_include")?;
        Ok(Self {
            outcome,
            family: optional_object_string(object, "family")?,
            family_prefix: optional_object_string(object, "family_prefix")?,
            family_any_of,
            candidates_include,
            unknown_reason: optional_object_string(object, "unknown_reason")?,
            route: optional_object_string(object, "route")?,
            alignment_status: optional_object_string(object, "alignment_status")?,
        })
    }

    fn to_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(outcome) = self.outcome {
            map.insert("outcome".to_string(), outcome.as_str().into());
        }
        if let Some(alignment_status) = &self.alignment_status {
            map.insert(
                "alignment_status".to_string(),
                alignment_status.clone().into(),
            );
        }
        if let Some(family) = &self.family {
            map.insert("family".to_string(), family.clone().into());
        }
        if let Some(prefix) = &self.family_prefix {
            map.insert("family_prefix".to_string(), prefix.clone().into());
        }
        if let Some(list) = &self.family_any_of {
            map.insert("family_any_of".to_string(), list.clone().into());
        }
        if let Some(list) = &self.candidates_include {
            map.insert("candidates_include".to_string(), list.clone().into());
        }
        if let Some(reason) = &self.unknown_reason {
            map.insert("unknown_reason".to_string(), reason.clone().into());
        }
        if let Some(route) = &self.route {
            map.insert("route".to_string(), route.clone().into());
        }
        serde_json::Value::Object(map)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvalActual {
    outcome: EvalOutcome,
    route: Option<String>,
    selected_family: Option<String>,
    candidate_family_count: usize,
    candidate_families: Vec<String>,
    /// Count of families the product hydrated before selecting/abstaining, and
    /// count of retrieval-pipeline stages executed. Both are read directly from
    /// `query_route` and stay `None` until the product surfaces them; a later
    /// wave adds the fields, at which point they populate without a schema bump.
    hydrated_family_count: Option<u64>,
    retrieval_stage_count: Option<u64>,
    unknown_reason: Option<String>,
    active_generation: Option<String>,
    /// The static-alignment certificate's `alignment_status` token, when the
    /// query drove the `check` operation. `None` for other operations.
    alignment_status: Option<String>,
}

impl EvalActual {
    fn from_query_json(value: &serde_json::Value) -> Result<Self, String> {
        let status = value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "query output omitted status".to_string())?;
        let query_route = value.get("query_route");
        let route = query_route
            .and_then(|route| route.get("route"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let selected_family = query_route
            .and_then(|route| route.get("selected_family_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let candidate_families = query_route
            .and_then(|route| route.get("candidate_family_ids"))
            .and_then(serde_json::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|entry| entry.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let unknown_reason = value
            .get("unknowns")
            .and_then(serde_json::Value::as_array)
            .and_then(|unknowns| unknowns.first())
            .and_then(|first| first.get("reason"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let hydrated_family_count = query_route
            .and_then(|route| route.get("hydrated_family_count"))
            .and_then(serde_json::Value::as_u64);
        let retrieval_stage_count = query_route
            .and_then(|route| route.get("retrieval_stage_count"))
            .and_then(serde_json::Value::as_u64);
        let active_generation = value
            .get("active_generation")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let alignment_status = value
            .get("alignment_status")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        Ok(Self {
            outcome: EvalOutcome::classify_status(status),
            route,
            selected_family,
            candidate_family_count: candidate_families.len(),
            candidate_families,
            hydrated_family_count,
            retrieval_stage_count,
            unknown_reason,
            active_generation,
            alignment_status,
        })
    }

    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "outcome": self.outcome.as_str(),
            "route": self.route,
            "selected_family": self.selected_family,
            "candidate_family_count": self.candidate_family_count,
            "candidate_families": self.candidate_families,
            "hydrated_family_count": self.hydrated_family_count,
            "retrieval_stage_count": self.retrieval_stage_count,
            "unknown_reason": self.unknown_reason,
            "active_generation": self.active_generation,
            "alignment_status": self.alignment_status,
        })
    }
}

#[derive(Debug, Clone)]
struct EvalMutation {
    kind: String,
    path: String,
    line: String,
}

#[derive(Debug, Clone)]
struct EvalQuery {
    query_id: String,
    fixture_id: String,
    kind: String,
    /// Optional measurement intent (`retrieval`/`abstention`/`context`). Parsed
    /// as a new optional field so the corpus schema stays `product-eval-corpus.v1`
    /// and legacy corpora without it still parse; intent-partitioned metrics only
    /// count queries that declare an intent.
    intent: Option<EvalIntent>,
    operation: EvalOperation,
    target: String,
    mode: String,
    mutation: Option<EvalMutation>,
    expected: EvalExpected,
}

impl EvalQuery {
    fn from_value(value: &serde_json::Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "corpus query must be an object".to_string())?;
        let mutation = match object.get("mutation") {
            None | Some(serde_json::Value::Null) => None,
            Some(mutation) => {
                let mutation = mutation
                    .as_object()
                    .ok_or_else(|| "query mutation must be an object".to_string())?;
                Some(EvalMutation {
                    kind: required_object_string(mutation, "kind")?,
                    path: required_object_string(mutation, "path")?,
                    line: required_object_string(mutation, "line")?,
                })
            }
        };
        let expected = EvalExpected::from_value(
            object
                .get("expected")
                .ok_or_else(|| "corpus query omitted expected".to_string())?,
        )?;
        let intent = match optional_object_string(object, "intent")? {
            None => None,
            Some(token) => Some(EvalIntent::from_token(&token)?),
        };
        Ok(Self {
            query_id: required_object_string(object, "query_id")?,
            fixture_id: required_object_string(object, "fixture_id")?,
            kind: required_object_string(object, "kind")?,
            intent,
            operation: EvalOperation::from_token(&required_object_string(object, "operation")?)?,
            target: required_object_string(object, "target")?,
            mode: optional_object_string(object, "mode")?.unwrap_or_else(|| "compact".to_string()),
            mutation,
            expected,
        })
    }
}

#[derive(Debug, Clone)]
struct EvalCorpusFixture {
    fixture_id: String,
    root: String,
}

#[derive(Debug, Clone)]
struct EvalCorpus {
    fixtures: Vec<EvalCorpusFixture>,
    queries: Vec<EvalQuery>,
}

impl EvalCorpus {
    fn from_value(value: &serde_json::Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "corpus must be a JSON object".to_string())?;
        let schema = object
            .get("schema_version")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "corpus omitted schema_version".to_string())?;
        if schema != PRODUCT_EVAL_CORPUS_SCHEMA {
            return Err(format!("unsupported corpus schema_version '{schema}'"));
        }
        let fixtures_value = object
            .get("fixtures")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "corpus omitted fixtures array".to_string())?;
        let mut fixtures = Vec::with_capacity(fixtures_value.len());
        for fixture in fixtures_value {
            let fixture = fixture
                .as_object()
                .ok_or_else(|| "corpus fixture must be an object".to_string())?;
            // `description` is validated as an optional string when present but is
            // not retained: the results schema does not surface fixture prose.
            let _ = optional_object_string(fixture, "description")?;
            fixtures.push(EvalCorpusFixture {
                fixture_id: required_object_string(fixture, "fixture_id")?,
                root: required_object_string(fixture, "root")?,
            });
        }
        let queries_value = object
            .get("queries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "corpus omitted queries array".to_string())?;
        let mut queries = Vec::with_capacity(queries_value.len());
        for query in queries_value {
            queries.push(EvalQuery::from_value(query)?);
        }
        if queries.is_empty() {
            return Err("corpus contains no queries".to_string());
        }
        for query in &queries {
            if !fixtures
                .iter()
                .any(|fixture| fixture.fixture_id == query.fixture_id)
            {
                return Err(format!(
                    "query '{}' references unknown fixture '{}'",
                    query.query_id, query.fixture_id
                ));
            }
        }
        Ok(Self { fixtures, queries })
    }
}

fn required_object_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, String> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing string field '{key}'"))
}

fn optional_object_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => Ok(Some(
            value
                .as_str()
                .ok_or_else(|| format!("field '{key}' must be a string"))?
                .to_string(),
        )),
    }
}

fn optional_string_array(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<Vec<String>>, String> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(array) => {
            let array = array
                .as_array()
                .ok_or_else(|| format!("field '{key}' must be an array"))?;
            let mut values = Vec::with_capacity(array.len());
            for entry in array {
                values.push(
                    entry
                        .as_str()
                        .ok_or_else(|| format!("field '{key}' entries must be strings"))?
                        .to_string(),
                );
            }
            Ok(Some(values))
        }
    }
}

fn evaluate_match(expected: &EvalExpected, actual: &EvalActual) -> (bool, Vec<String>) {
    let mut mismatches = Vec::new();
    if let Some(outcome) = expected.outcome {
        if actual.outcome != outcome {
            mismatches.push("outcome".to_string());
        }
    }
    if let Some(family) = &expected.family {
        if actual.selected_family.as_deref() != Some(family.as_str()) {
            mismatches.push("family".to_string());
        }
    }
    if let Some(prefix) = &expected.family_prefix {
        if !actual
            .selected_family
            .as_deref()
            .is_some_and(|selected| selected.starts_with(prefix.as_str()))
        {
            mismatches.push("family_prefix".to_string());
        }
    }
    if let Some(prefixes) = &expected.family_any_of {
        if !actual.selected_family.as_deref().is_some_and(|selected| {
            prefixes
                .iter()
                .any(|prefix| selected.starts_with(prefix.as_str()))
        }) {
            mismatches.push("family_any_of".to_string());
        }
    }
    if let Some(reason) = &expected.unknown_reason {
        if actual.unknown_reason.as_deref() != Some(reason.as_str()) {
            mismatches.push("unknown_reason".to_string());
        }
    }
    if let Some(route) = &expected.route {
        if actual.route.as_deref() != Some(route.as_str()) {
            mismatches.push("route".to_string());
        }
    }
    if let Some(alignment_status) = &expected.alignment_status {
        if actual.alignment_status.as_deref() != Some(alignment_status.as_str()) {
            mismatches.push("alignment_status".to_string());
        }
    }
    (mismatches.is_empty(), mismatches)
}

/// True when a family was selected but the query's expected family/prefix
/// constraints exclude it. Queries without a family constraint never count.
fn is_false_family_selection(expected: &EvalExpected, actual: &EvalActual) -> bool {
    let Some(selected) = actual.selected_family.as_deref() else {
        return false;
    };
    let mut has_constraint = false;
    let mut satisfied = true;
    if let Some(family) = &expected.family {
        has_constraint = true;
        if selected != family.as_str() {
            satisfied = false;
        }
    }
    if let Some(prefix) = &expected.family_prefix {
        has_constraint = true;
        if !selected.starts_with(prefix.as_str()) {
            satisfied = false;
        }
    }
    if let Some(prefixes) = &expected.family_any_of {
        has_constraint = true;
        if !prefixes
            .iter()
            .any(|prefix| selected.starts_with(prefix.as_str()))
        {
            satisfied = false;
        }
    }
    has_constraint && !satisfied
}

/// True when the query declares at least one family constraint
/// (`family`/`family_prefix`/`family_any_of`).
fn has_family_constraint(expected: &EvalExpected) -> bool {
    expected.family.is_some()
        || expected.family_prefix.is_some()
        || expected.family_any_of.is_some()
}

/// True when `id` satisfies every declared family constraint. Mirrors the
/// per-field match semantics: exact `family`, `family_prefix` by prefix, and
/// `family_any_of` by any prefix; all present constraints must hold. Returns
/// false when no family constraint is declared, so a gold hit always requires an
/// explicit family target.
fn id_satisfies_family_gold(expected: &EvalExpected, id: &str) -> bool {
    let mut present = false;
    if let Some(family) = &expected.family {
        present = true;
        if id != family.as_str() {
            return false;
        }
    }
    if let Some(prefix) = &expected.family_prefix {
        present = true;
        if !id.starts_with(prefix.as_str()) {
            return false;
        }
    }
    if let Some(prefixes) = &expected.family_any_of {
        present = true;
        if !prefixes
            .iter()
            .any(|prefix| id.starts_with(prefix.as_str()))
        {
            return false;
        }
    }
    present
}

/// True when the selected family satisfies the query's family gold. This is the
/// Hit@1 predicate: a single supported selection that matches the gold family.
fn selected_satisfies_family_gold(expected: &EvalExpected, actual: &EvalActual) -> bool {
    actual
        .selected_family
        .as_deref()
        .is_some_and(|selected| id_satisfies_family_gold(expected, selected))
}

/// Reciprocal rank of the committed answer. MRR credits only a run that commits
/// (an `ok`/`partial_context` outcome): a selection satisfying gold is rank 1, and
/// otherwise the first gold-satisfying id within the top `RETRIEVAL_CANDIDATE_K`
/// candidates (1-based) sets the rank. A run that abstains (`unknown`/`fallback`)
/// scores 0 regardless of its diagnostic candidate list, and a query with no
/// family constraint scores 0. List construction is measured by `candidate_recall`,
/// not here.
fn reciprocal_rank(expected: &EvalExpected, actual: &EvalActual) -> f64 {
    if !has_family_constraint(expected) {
        return 0.0;
    }
    if matches!(actual.outcome, EvalOutcome::Unknown | EvalOutcome::Fallback) {
        return 0.0;
    }
    if selected_satisfies_family_gold(expected, actual) {
        return 1.0;
    }
    for (index, candidate) in actual
        .candidate_families
        .iter()
        .take(RETRIEVAL_CANDIDATE_K)
        .enumerate()
    {
        if id_satisfies_family_gold(expected, candidate) {
            return 1.0 / (index as f64 + 1.0);
        }
    }
    0.0
}

/// True when every `candidates_include` prefix is matched by some candidate family
/// within the top `RETRIEVAL_CANDIDATE_K`. Empty gold vacuously holds, but such
/// queries are excluded from the candidate-recall denominator by the caller.
fn candidate_recall_satisfied(includes: &[String], candidates: &[String]) -> bool {
    includes.iter().all(|include| {
        candidates
            .iter()
            .take(RETRIEVAL_CANDIDATE_K)
            .any(|candidate| candidate.starts_with(include.as_str()))
    })
}

/// A token-overlap selection: the family the baseline chose (if any) and its
/// own ranked candidate list (already capped at `RETRIEVAL_CANDIDATE_K`).
struct BaselineSelection {
    selected_family: Option<String>,
    candidate_families: Vec<String>,
}

/// Lowercases `target`, splits on non-ASCII-alphanumeric characters, drops tokens
/// shorter than `BASELINE_MIN_TOKEN_LEN` characters, and deduplicates preserving
/// first occurrence. Deterministic for a given input.
fn baseline_tokenize(target: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for piece in target
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
    {
        if piece.chars().count() < BASELINE_MIN_TOKEN_LEN {
            continue;
        }
        let token = piece.to_string();
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    tokens
}

/// Number of distinct query tokens that occur as substrings of `family_id`
/// (case-insensitive). `tokens` are already lowercased and deduplicated, so the
/// count is bounded by `tokens.len()`.
fn baseline_family_score(tokens: &[String], family_id: &str) -> usize {
    let lowered = family_id.to_lowercase();
    tokens
        .iter()
        .filter(|token| lowered.contains(token.as_str()))
        .count()
}

/// Deterministic token-overlap selection. Scores every family, ranks families
/// with a positive score by score descending then family id ascending (capped at
/// `RETRIEVAL_CANDIDATE_K`), and selects the unique argmax only when its score is
/// at least `BASELINE_SELECT_MIN_SCORE`. A strict tie at the maximum or a
/// sub-threshold maximum abstains. No aliases, concepts, or margin calibration.
fn baseline_select_family(tokens: &[String], family_ids: &[String]) -> BaselineSelection {
    let mut scored: Vec<(usize, &String)> = family_ids
        .iter()
        .map(|id| (baseline_family_score(tokens, id), id))
        .filter(|(score, _)| *score >= 1)
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(right.1)));
    let candidate_families: Vec<String> = scored
        .iter()
        .take(RETRIEVAL_CANDIDATE_K)
        .map(|(_, id)| (*id).clone())
        .collect();
    let max_score = scored.first().map(|(score, _)| *score).unwrap_or(0);
    let top_count = scored
        .iter()
        .filter(|(score, _)| *score == max_score)
        .count();
    let selected_family = if max_score >= BASELINE_SELECT_MIN_SCORE && top_count == 1 {
        scored.first().map(|(_, id)| (*id).clone())
    } else {
        None
    };
    BaselineSelection {
        selected_family,
        candidate_families,
    }
}

/// Projects a baseline selection onto the same `EvalActual` shape the product
/// path produces: a selection reports `ok`, an abstention reports `unknown`. The
/// baseline has no route, typed unknown reason, or pipeline counters, so those
/// stay `None`; `active_generation` carries the generation of the family listing.
fn baseline_actual(selection: BaselineSelection, active_generation: Option<String>) -> EvalActual {
    let outcome = if selection.selected_family.is_some() {
        EvalOutcome::Ok
    } else {
        EvalOutcome::Unknown
    };
    EvalActual {
        outcome,
        route: None,
        selected_family: selection.selected_family,
        candidate_family_count: selection.candidate_families.len(),
        candidate_families: selection.candidate_families,
        hydrated_family_count: None,
        retrieval_stage_count: None,
        unknown_reason: None,
        active_generation,
        alignment_status: None,
    }
}

/// Runs the token-overlap baseline for one query against a fixture's family
/// listing. Selection is deterministic, so the repetition loop only records
/// scoring latencies for schema parity with the product path.
fn run_baseline_query(
    query: &EvalQuery,
    active_generation: Option<&str>,
    family_ids: &[String],
    repetitions: usize,
) -> (EvalActual, Vec<u128>) {
    let tokens = baseline_tokenize(&query.target);
    let mut latencies = Vec::with_capacity(repetitions);
    let mut selection = BaselineSelection {
        selected_family: None,
        candidate_families: Vec::new(),
    };
    for _ in 0..repetitions {
        let started = std::time::Instant::now();
        selection = baseline_select_family(&tokens, family_ids);
        latencies.push(started.elapsed().as_millis());
    }
    let actual = baseline_actual(selection, active_generation.map(str::to_string));
    (actual, latencies)
}

/// One evaluated query reduced to the fields the retrieval metrics need.
struct EvalMetricRecord<'a> {
    intent: Option<EvalIntent>,
    kind: &'a str,
    expected: &'a EvalExpected,
    actual: &'a EvalActual,
    is_match: bool,
}

/// Retrieval-quality metrics derived from the intent taxonomy. Every rate keeps
/// its integer numerator/denominator so a reader can audit it without trusting
/// the float, and a rate over an empty denominator serializes as `null`.
#[derive(Debug, Default, PartialEq)]
struct ProductEvalMetrics {
    hit_at_1_num: usize,
    hit_at_1_den: usize,
    candidate_recall_num: usize,
    candidate_recall_den: usize,
    mrr_sum: f64,
    mrr_den: usize,
    correct_abstention_num: usize,
    correct_abstention_den: usize,
    false_family_selections: usize,
    family_constrained_total: usize,
    /// Safety counter: queries whose gold outcome is `unknown` (a family should not
    /// be committed) where the run nonetheless selected a family. Independent of
    /// `false_family_selections`, which needs a declared family constraint; an
    /// abstention-gold query carries none, so a confident wrong selection there is
    /// invisible to `false_family_selections` but counted here.
    selected_on_abstention_gold: usize,
    unsupported_rejection_num: usize,
    unsupported_rejection_den: usize,
    ambiguity_precision_num: usize,
    ambiguity_precision_den: usize,
    by_intent: std::collections::BTreeMap<&'static str, (usize, usize)>,
}

fn ratio_value(num: usize, den: usize) -> serde_json::Value {
    if den == 0 {
        serde_json::Value::Null
    } else {
        serde_json::json!(num as f64 / den as f64)
    }
}

fn mean_value(sum: f64, den: usize) -> serde_json::Value {
    if den == 0 {
        serde_json::Value::Null
    } else {
        serde_json::json!(sum / den as f64)
    }
}

fn format_mean(sum: f64, den: usize) -> String {
    if den == 0 {
        "n/a".to_string()
    } else {
        format!("{:.3}", sum / den as f64)
    }
}

fn compute_product_eval_metrics(records: &[EvalMetricRecord]) -> ProductEvalMetrics {
    let mut metrics = ProductEvalMetrics::default();
    for record in records {
        if let Some(intent) = record.intent {
            let counters = metrics.by_intent.entry(intent.as_str()).or_insert((0, 0));
            counters.0 += 1;
            if record.is_match {
                counters.1 += 1;
            }
        }
        if has_family_constraint(record.expected) {
            metrics.family_constrained_total += 1;
            if is_false_family_selection(record.expected, record.actual) {
                metrics.false_family_selections += 1;
            }
        }
        if record.expected.outcome == Some(EvalOutcome::Unknown)
            && record.actual.selected_family.is_some()
        {
            metrics.selected_on_abstention_gold += 1;
        }
        if let Some(includes) = &record.expected.candidates_include {
            if !includes.is_empty() {
                metrics.candidate_recall_den += 1;
                if candidate_recall_satisfied(includes, &record.actual.candidate_families) {
                    metrics.candidate_recall_num += 1;
                }
            }
        }
        if record.kind == "unsupported_concept" {
            metrics.unsupported_rejection_den += 1;
            if record.actual.outcome == EvalOutcome::Unknown {
                metrics.unsupported_rejection_num += 1;
            }
        }
        match record.intent {
            Some(EvalIntent::Retrieval) => {
                metrics.hit_at_1_den += 1;
                if selected_satisfies_family_gold(record.expected, record.actual) {
                    metrics.hit_at_1_num += 1;
                }
                metrics.mrr_den += 1;
                metrics.mrr_sum += reciprocal_rank(record.expected, record.actual);
            }
            Some(EvalIntent::Abstention) => {
                metrics.correct_abstention_den += 1;
                if record.actual.outcome == EvalOutcome::Unknown {
                    metrics.correct_abstention_num += 1;
                }
                if record.kind == "ambiguous" || record.kind == "nl_pattern_question" {
                    metrics.ambiguity_precision_den += 1;
                    if record.actual.outcome == EvalOutcome::Unknown {
                        metrics.ambiguity_precision_num += 1;
                    }
                }
            }
            Some(EvalIntent::Context) | None => {}
        }
    }
    metrics
}

impl ProductEvalMetrics {
    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "hit_at_1": ratio_value(self.hit_at_1_num, self.hit_at_1_den),
            "hit_at_1_counts": { "num": self.hit_at_1_num, "den": self.hit_at_1_den },
            "candidate_recall": ratio_value(self.candidate_recall_num, self.candidate_recall_den),
            "candidate_recall_counts": {
                "num": self.candidate_recall_num,
                "den": self.candidate_recall_den,
            },
            "mrr": mean_value(self.mrr_sum, self.mrr_den),
            "mrr_counts": { "den": self.mrr_den },
            "correct_abstention_rate": ratio_value(
                self.correct_abstention_num,
                self.correct_abstention_den,
            ),
            "correct_abstention_counts": {
                "num": self.correct_abstention_num,
                "den": self.correct_abstention_den,
            },
            "false_family_rate": ratio_value(
                self.false_family_selections,
                self.family_constrained_total,
            ),
            "false_family_selections": self.false_family_selections,
            "family_constrained_total": self.family_constrained_total,
            "selected_on_abstention_gold": self.selected_on_abstention_gold,
            "unsupported_rejection_rate": ratio_value(
                self.unsupported_rejection_num,
                self.unsupported_rejection_den,
            ),
            "unsupported_rejection_counts": {
                "num": self.unsupported_rejection_num,
                "den": self.unsupported_rejection_den,
            },
            "ambiguity_precision": ratio_value(
                self.ambiguity_precision_num,
                self.ambiguity_precision_den,
            ),
            "ambiguity_precision_counts": {
                "num": self.ambiguity_precision_num,
                "den": self.ambiguity_precision_den,
            },
        })
    }
}

fn percentile(values: &[u128], percentile: u8) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (percentile as usize * sorted.len()).div_ceil(100);
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

fn unix_seconds_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn rfc3339_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn fixture_version_hash(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_fixture_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (relative, absolute) in &files {
        hasher.update((relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        let contents = fs::read(absolute)
            .map_err(|_| "could not read fixture file for hashing".to_string())?;
        hasher.update((contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn collect_fixture_files(
    base: &Path,
    current: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(current)
        .map_err(|_| "could not read fixture directory for hashing".to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for entry in entries {
        let metadata = fs::symlink_metadata(&entry)
            .map_err(|_| "could not inspect fixture entry for hashing".to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_fixture_files(base, &entry, out)?;
        } else if metadata.is_file() {
            let relative = entry
                .strip_prefix(base)
                .map_err(|_| "fixture path escaped the fixture root".to_string())?;
            let relative = relative
                .to_str()
                .ok_or_else(|| "fixture path was not UTF-8".to_string())?
                .replace('\\', "/");
            out.push((relative, entry));
        }
    }
    Ok(())
}

fn copy_dir_sorted(src: &Path, dst: &Path) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(src)
        .map_err(|_| "could not read fixture directory".to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for entry in entries {
        let name = entry
            .file_name()
            .ok_or_else(|| "fixture entry has no file name".to_string())?;
        let target = dst.join(name);
        let metadata = fs::symlink_metadata(&entry)
            .map_err(|_| "could not inspect fixture entry".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("fixture contains a symlink, which is not supported".to_string());
        }
        if metadata.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|_| "could not create fixture subdirectory".to_string())?;
            copy_dir_sorted(&entry, &target)?;
        } else if metadata.is_file() {
            fs::copy(&entry, &target).map_err(|_| "could not copy fixture file".to_string())?;
        }
    }
    Ok(())
}

fn unique_product_eval_root() -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    env::temp_dir().join(format!(
        "repogrammar-product-eval-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

struct ResyncStats {
    latency_ms: u128,
    discovered_files: Option<u64>,
    stored_files: Option<u64>,
}

struct EvalWorkspace {
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
    tools: PathBuf,
    binary: PathBuf,
    python: PathBuf,
    cleanup: std::cell::Cell<bool>,
}

impl EvalWorkspace {
    fn new(binary: &Path, fixture_source: &Path) -> Result<Self, String> {
        let root = unique_product_eval_root();
        let home = root.join("home");
        let project = root.join("project");
        let tools = root.join("tools");
        fs::create_dir_all(&home).map_err(|_| "could not create isolated eval HOME".to_string())?;
        fs::create_dir_all(&project)
            .map_err(|_| "could not create isolated eval project".to_string())?;
        fs::create_dir_all(&tools)
            .map_err(|_| "could not create isolated eval tool PATH".to_string())?;
        let python = prepare_isolated_tool_path(&tools)?;
        copy_dir_sorted(fixture_source, &project)?;
        Ok(Self {
            root,
            home,
            project,
            tools,
            binary: binary.to_path_buf(),
            python,
            cleanup: std::cell::Cell::new(true),
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
            .env("REPOGRAMMAR_PYTHON_EXECUTABLE", &self.python)
            .env("REPOGRAMMAR_TELEMETRY", "0")
            .env("DO_NOT_TRACK", "1");
        command
    }

    fn project_arg(&self) -> Result<String, String> {
        self.project
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| "isolated eval project path was not UTF-8".to_string())
    }

    fn init(&self) -> Result<(), String> {
        let project = self.project_arg()?;
        let output = self
            .command()
            .args([
                "init",
                "--project",
                &project,
                "--yes",
                "--json",
                "--progress",
                "never",
            ])
            .output()
            .map_err(|_| "eval init could not execute".to_string())?;
        if !output.status.success() {
            return Err("eval init returned a failure status".to_string());
        }
        Ok(())
    }

    fn resync(&self) -> Result<ResyncStats, String> {
        let project = self.project_arg()?;
        let started = std::time::Instant::now();
        let output = self
            .command()
            .args([
                "resync",
                "--project",
                &project,
                "--json",
                "--progress",
                "never",
            ])
            .output()
            .map_err(|_| "eval resync could not execute".to_string())?;
        let latency_ms = started.elapsed().as_millis();
        if !output.status.success() {
            return Err("eval resync returned a failure status".to_string());
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| "eval resync output was not UTF-8".to_string())?;
        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|_| "eval resync output was not JSON".to_string())?;
        if value.get("status").and_then(serde_json::Value::as_str) != Some("complete") {
            return Err("eval resync did not complete".to_string());
        }
        Ok(ResyncStats {
            latency_ms,
            discovered_files: value
                .get("discovered_files")
                .and_then(serde_json::Value::as_u64),
            stored_files: value
                .get("stored_files")
                .and_then(serde_json::Value::as_u64),
        })
    }

    /// Fetches the product's `families --json` listing once for an indexed
    /// fixture. Returns the active generation and every reported `family_id`;
    /// an empty (typed-`UNKNOWN`) listing yields an empty vector rather than an
    /// error, so the baseline abstains everywhere for that fixture.
    fn families(&self) -> Result<(Option<String>, Vec<String>), String> {
        let project = self.project_arg()?;
        let output = self
            .command()
            .args(["families", "--project", &project, "--json"])
            .output()
            .map_err(|_| "eval families listing could not execute".to_string())?;
        if !output.status.success() {
            return Err("eval families listing returned a failure status".to_string());
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| "eval families listing output was not UTF-8".to_string())?;
        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|_| "eval families listing output was not JSON".to_string())?;
        let active_generation = value
            .get("active_generation")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let family_ids = value
            .get("families")
            .and_then(serde_json::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|entry| {
                        entry
                            .get("family_id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok((active_generation, family_ids))
    }

    fn apply_mutation(&self, mutation: &EvalMutation) -> Result<(), String> {
        if mutation.kind != "append_line" {
            return Err(format!("unsupported mutation kind '{}'", mutation.kind));
        }
        let relative = Path::new(&mutation.path);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err("mutation path must be a normalized relative path".to_string());
        }
        let target = self.project.join(relative);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&target)
            .map_err(|_| "could not open fixture file for mutation".to_string())?;
        writeln!(file, "\n{}", mutation.line).map_err(|_| "could not apply mutation".to_string())
    }

    fn run_query(
        &self,
        query: &EvalQuery,
        repetitions: usize,
    ) -> Result<(EvalActual, Vec<u128>), String> {
        let project = self.project_arg()?;
        let mut latencies = Vec::with_capacity(repetitions);
        let mut last_stdout = None;
        for _ in 0..repetitions {
            let started = std::time::Instant::now();
            let output = self
                .command()
                .args([
                    query.operation.as_str(),
                    query.target.as_str(),
                    "--project",
                    &project,
                    "--mode",
                    query.mode.as_str(),
                    "--json",
                ])
                .output()
                .map_err(|_| format!("query '{}' could not execute", query.query_id))?;
            latencies.push(started.elapsed().as_millis());
            last_stdout = Some(
                String::from_utf8(output.stdout)
                    .map_err(|_| format!("query '{}' output was not UTF-8", query.query_id))?,
            );
        }
        let stdout = last_stdout
            .ok_or_else(|| format!("query '{}' produced no repetitions", query.query_id))?;
        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|_| format!("query '{}' output was not JSON", query.query_id))?;
        let actual = EvalActual::from_query_json(&value)?;
        Ok((actual, latencies))
    }

    /// Captures one query response as the verbatim CLI `--json` payload plus its
    /// exact serialized byte length, threading the requested `verbosity` tier.
    /// Unlike [`EvalWorkspace::run_query`] it performs no matching or
    /// repetitions and does not require a success exit status, so abstention,
    /// partial-context, and insufficient-evidence shapes (which the product
    /// emits on stdout) are measured verbatim. The byte length is the trimmed
    /// payload length, excluding any trailing newline, so it is stable across
    /// runs of the same fixture and binary. Used only by the payload-measure
    /// harness; the resolution decision is unchanged.
    fn capture_query_json(
        &self,
        operation: &str,
        target: &str,
        mode: &str,
        verbosity: &str,
        include_source_spans: bool,
    ) -> Result<(serde_json::Value, usize), String> {
        let project = self.project_arg()?;
        let mut command = self.command();
        command.args([
            operation,
            target,
            "--project",
            &project,
            "--mode",
            mode,
            "--verbosity",
            verbosity,
            "--json",
        ]);
        if include_source_spans {
            command.arg("--include-source-spans");
        }
        let output = command.output().map_err(|_| {
            format!("payload-measure query '{operation} {target}' could not execute")
        })?;
        let stdout = String::from_utf8(output.stdout).map_err(|_| {
            format!("payload-measure query '{operation} {target}' output was not UTF-8")
        })?;
        let trimmed = stdout.trim();
        let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|_| {
            format!("payload-measure query '{operation} {target}' output was not JSON")
        })?;
        Ok((value, trimmed.len()))
    }

    /// Drives the product's MCP `serve` stdio surface for a single
    /// `inspect_readiness` call and returns the bounded, source-free readiness
    /// payload plus its serialized byte length. This measures the actual MCP
    /// readiness surface (lean by construction) rather than the CLI `status`
    /// lifecycle command, whose storage internals (`wal_bytes`/`shm_bytes`/...)
    /// are volatile and out of scope for the response-precision policy.
    fn capture_inspect_readiness(&self) -> Result<(serde_json::Value, usize), String> {
        let project = self.project_arg()?;
        let mut child = self
            .command()
            .args(["serve", "--project", &project])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| "payload-measure could not start MCP serve".to_string())?;
        let requests = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":{}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":\
{\"name\":\"repogrammar_context\",\"arguments\":{\"operation\":\"inspect_readiness\"}}}\n"
        );
        child
            .stdin
            .take()
            .ok_or_else(|| "payload-measure MCP serve stdin was unavailable".to_string())?
            .write_all(requests.as_bytes())
            .map_err(|_| "payload-measure could not write the MCP serve request".to_string())?;
        let output = child
            .wait_with_output()
            .map_err(|_| "payload-measure MCP serve did not complete".to_string())?;
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| "payload-measure MCP serve output was not UTF-8".to_string())?;
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let message: serde_json::Value = match serde_json::from_str(line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if message.get("id").and_then(serde_json::Value::as_u64) != Some(1) {
                continue;
            }
            let text = message
                .get("result")
                .and_then(|result| result.get("content"))
                .and_then(serde_json::Value::as_array)
                .and_then(|content| content.first())
                .and_then(|entry| entry.get("text"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "MCP inspect_readiness response had no text content".to_string())?;
            let value: serde_json::Value = serde_json::from_str(text)
                .map_err(|_| "MCP inspect_readiness content was not JSON".to_string())?;
            return Ok((value, text.len()));
        }
        Err("MCP inspect_readiness produced no tools/call response".to_string())
    }

    fn retain(&self) {
        if self.cleanup.replace(false) {
            eprintln!(
                "product-eval workspace retained for inspection: {}",
                self.root.display()
            );
        }
    }
}

impl Drop for EvalWorkspace {
    fn drop(&mut self) {
        if self.cleanup.get() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn retain_workspaces(workspaces: &[(String, EvalWorkspace)]) {
    for (_, workspace) in workspaces {
        workspace.retain();
    }
}

fn resolve_product_binary(root: &Path, bin_override: Option<&str>) -> Result<PathBuf, String> {
    if let Some(explicit) = bin_override {
        return regular_input_file(root, explicit, "repogrammar binary");
    }
    let current = env::current_exe()
        .map_err(|_| "could not resolve the running executable path".to_string())?;
    let directory = current
        .parent()
        .ok_or_else(|| "running executable has no parent directory".to_string())?;
    let sibling = directory.join("repogrammar");
    let metadata = fs::symlink_metadata(&sibling)
        .map_err(|_| "sibling repogrammar binary not found; pass --bin <path>".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(
            "sibling repogrammar binary is not a regular file; pass --bin <path>".to_string(),
        );
    }
    fs::canonicalize(&sibling)
        .map_err(|_| "sibling repogrammar binary is unavailable; pass --bin <path>".to_string())
}

fn corpus_relative_path(root: &Path, absolute: &Path, original: &str) -> String {
    if let Ok(relative) = absolute.strip_prefix(root) {
        return relative.to_string_lossy().replace('\\', "/");
    }
    if !Path::new(original).is_absolute() {
        return original.replace('\\', "/");
    }
    original.to_string()
}

// ===========================================================================
// Phase 4 S0: incremental/full-build equivalence oracle.
//
// `sync-equivalence` proves that a dependency-gated incremental `sync` produces
// a semantically identical active generation to a clean full rebuild over the
// same worktree, or explicitly falls back to a full rebuild. It drives the
// product `repogrammar` binary in isolated workspaces (identical env to
// `product-eval`) and compares canonical dumps of the product's own read
// surfaces plus the semantic-fact ledger.
//
// Canonicalization strips only the sanctioned non-semantic fields: generation
// ids/timestamps (never surfaced into the compared dumps — the top-level
// `active_generation` of every response and the `unknown_inventory`'s
// `active_generation` are dropped) and the order/history-assigned
// `fact_id`/`evidence_id` sequence numbers (excluded from the fact content
// tuple). The single sanctioned semantic divergence — provider/worker-origin
// facts retained by a worker-less incremental sync for unchanged files — is
// encoded explicitly: such facts are checked against the retention rule
// (path unchanged and content hash matching the current indexed file) instead
// of by equality with the clean rebuild. Every v1 scenario is worker-less, so
// that provider bucket is empty by construction; the rule is nonetheless
// applied rather than blanket-ignored.
// ===========================================================================

/// A scripted, deterministic patch applied identically to the incremental
/// workspace (after its base build) and the clean-rebuild workspace, together
/// with the outcome the oracle demands. A scenario passes only when the observed
/// outcome and (for fallbacks) the fallback reason match these expectations, so
/// an unexpected `EQUAL` (a gate that silently regressed to the incremental
/// path), an unexpected fallback (e.g. a preflight that misfires and falls back
/// everywhere), or a wrong fallback reason all exit non-zero.
struct SyncEquivalenceScenario {
    id: &'static str,
    patch_summary: &'static str,
    apply: fn(&Path) -> Result<(), String>,
    expected_outcome: &'static str,
    expected_fallback_reason: Option<&'static str>,
    /// When set, the observed `reparsed_files` in the sync report must match
    /// exactly. Used to prove a file-local incremental path reparsed only the
    /// edited file(s) rather than silently rebuilding more; `None` skips the
    /// check (e.g. every full-rebuild fallback, which reparses everything).
    expected_reparsed_files: Option<usize>,
}

const SYNC_EQUIVALENCE_SCENARIOS: &[SyncEquivalenceScenario] = &[
    SyncEquivalenceScenario {
        id: "java_edit",
        patch_summary: "modify a Java test-method body (file-local incremental path)",
        apply: patch_java_edit,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "csharp_edit",
        patch_summary: "modify a C# test-method body (file-local incremental path)",
        apply: patch_csharp_edit,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "docs_noop",
        patch_summary: "edit an undiscovered Markdown file (empty delta, no-op sync)",
        apply: patch_docs_noop,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "java_add",
        patch_summary: "add a new Java test file (incremental add path)",
        apply: patch_java_add,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "java_delete",
        patch_summary: "delete a Java test file (incremental remove path)",
        apply: patch_java_delete,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "rs_content_edit",
        patch_summary: "modify a Rust test-fn body (file-local incremental fast path)",
        apply: patch_rs_content_edit,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: Some(1),
    },
    SyncEquivalenceScenario {
        id: "tsjs_content_edit",
        patch_summary: "modify a TS ambient test body (file-local incremental fast path)",
        apply: patch_tsjs_content_edit,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        expected_reparsed_files: Some(1),
    },
    SyncEquivalenceScenario {
        id: "tsjs_add",
        patch_summary: "add a new TS test file (path set grows: fallback)",
        apply: patch_tsjs_add,
        expected_outcome: "FELL_BACK",
        expected_fallback_reason: Some("project_context_changed"),
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "rs_add",
        patch_summary: "add a new Rust source file (path set grows: fallback)",
        apply: patch_rs_add,
        expected_outcome: "FELL_BACK",
        expected_fallback_reason: Some("project_context_changed"),
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "mocharc_remove",
        patch_summary: "remove .mocharc.json, flipping the TS/JS test-runner flag (defect-1 fix)",
        apply: patch_mocharc_remove,
        expected_outcome: "FELL_BACK",
        expected_fallback_reason: Some("project_context_changed"),
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "python_body_edit",
        patch_summary:
            "modify a Python function body (interface unchanged: file-local incremental)",
        apply: patch_python_body_edit,
        expected_outcome: "EQUAL",
        expected_fallback_reason: None,
        // Only the edited module is reparsed; the sibling conftest copies forward.
        expected_reparsed_files: Some(1),
    },
    SyncEquivalenceScenario {
        id: "python_interface_edit",
        patch_summary: "add a top-level Python function (interface changes: fallback)",
        apply: patch_python_interface_edit,
        expected_outcome: "FELL_BACK",
        expected_fallback_reason: Some("python_interface_changed"),
        expected_reparsed_files: None,
    },
    SyncEquivalenceScenario {
        id: "python_conftest_edit",
        patch_summary:
            "modify a conftest.py body (fixture context: fallback regardless of interface)",
        apply: patch_python_conftest_edit,
        expected_outcome: "FELL_BACK",
        expected_fallback_reason: Some("project_context_changed"),
        expected_reparsed_files: None,
    },
];

fn sync_equivalence_replace_once(
    project: &Path,
    relative: &str,
    needle: &str,
    replacement: &str,
) -> Result<(), String> {
    let path = project.join(relative);
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("could not read scenario file '{relative}'"))?;
    let Some(position) = text.find(needle) else {
        return Err(format!("scenario needle not found in '{relative}'"));
    };
    let mut patched = String::with_capacity(text.len() + replacement.len());
    patched.push_str(&text[..position]);
    patched.push_str(replacement);
    patched.push_str(&text[position + needle.len()..]);
    fs::write(&path, patched).map_err(|_| format!("could not write scenario file '{relative}'"))
}

fn patch_java_edit(project: &Path) -> Result<(), String> {
    sync_equivalence_replace_once(
        project,
        "service/java/OrderServiceTest.java",
        "void placesOrder() {\n        assert true;",
        "void placesOrder() {\n        assert 1 == 1;",
    )
}

fn patch_csharp_edit(project: &Path) -> Result<(), String> {
    sync_equivalence_replace_once(
        project,
        "service/csharp/CatalogTests.cs",
        "public void ReturnsItems()\n    {\n        Assert.True(true);",
        "public void ReturnsItems()\n    {\n        Assert.True(1 == 1);",
    )
}

fn patch_docs_noop(project: &Path) -> Result<(), String> {
    let path = project.join("docs/NOTES.md");
    let mut text =
        fs::read_to_string(&path).map_err(|_| "could not read scenario file 'docs/NOTES.md'")?;
    text.push_str("\nAppended by the docs-only no-op scenario.\n");
    fs::write(&path, text).map_err(|_| "could not write scenario file 'docs/NOTES.md'".to_string())
}

fn patch_rs_content_edit(project: &Path) -> Result<(), String> {
    // Content-only edit of one Rust test-fn body. The Rust path set and (absent)
    // manifest are unchanged, so only this file is reparsed on the incremental
    // path; the recomputed family must match a clean rebuild exactly.
    sync_equivalence_replace_once(
        project,
        "service/rust/order_service.rs",
        "let outcome = place_order(\"alpha\");",
        "let outcome = place_order(\"renamed\");",
    )
}

fn patch_tsjs_content_edit(project: &Path) -> Result<(), String> {
    // Content-only edit of one TS ambient test body. TS parsing consumes only the
    // path set and root config, so the edit is file-local and stays incremental.
    sync_equivalence_replace_once(
        project,
        "web/users.test.ts",
        "it(\"loads users\", () => {});",
        "it(\"loads users\", () => { return; });",
    )
}

fn patch_tsjs_add(project: &Path) -> Result<(), String> {
    // Adding a TS file grows `tsjs_module_paths`, which can change how other files
    // resolve import specifiers, so the gate must fall back to a full rebuild.
    fs::write(
        project.join("web/payments.test.ts"),
        "describe(\"ambient payments\", () => {\n  it(\"loads payments\", () => {});\n  test(\"refunds payments\", () => {});\n});\n",
    )
    .map_err(|_| "could not add scenario file 'web/payments.test.ts'".to_string())
}

fn patch_rs_add(project: &Path) -> Result<(), String> {
    // Adding a Rust file grows `rust_module_paths`, which can change `mod`
    // candidate resolution for other files, so the gate must fall back.
    fs::write(
        project.join("service/rust/extra_service.rs"),
        "#[test]\nfn refunds_order() {\n    let outcome = settle_order(\"delta\");\n    assert!(outcome);\n}\n\nfn settle_order(_name: &str) -> bool {\n    true\n}\n",
    )
    .map_err(|_| "could not add scenario file 'service/rust/extra_service.rs'".to_string())
}

fn patch_mocharc_remove(project: &Path) -> Result<(), String> {
    // Removing the root .mocharc.json flips the global TS/JS test-runner flag
    // off. The ambient tests under web/ form runner families only while the flag
    // is on, so if the gate ever regressed to the incremental path B would copy
    // forward the stale flag-on families while the clean rebuild C would not,
    // producing a real inequality on top of the expected-outcome check.
    fs::remove_file(project.join(".mocharc.json"))
        .map_err(|_| "could not delete scenario file '.mocharc.json'".to_string())
}

fn patch_python_body_edit(project: &Path) -> Result<(), String> {
    // A function-body edit that leaves every top-level symbol, `__all__`, and
    // `__init__` re-export untouched: the module interface hash is stable, so the
    // edit is provably file-local. Only `analytics/app.py` reparses; its sibling
    // `analytics/conftest.py` copies forward, and the result must equal a clean
    // rebuild exactly.
    sync_equivalence_replace_once(
        project,
        "analytics/app.py",
        "return \"default\"",
        "return \"primary\"",
    )
}

fn patch_python_interface_edit(project: &Path) -> Result<(), String> {
    // Adding a top-level function changes `analytics/app.py`'s exported symbol
    // surface, so the interface hash changes and the preflight must fall back to
    // a full rebuild rather than reparse the file in isolation (another module
    // could resolve the new symbol).
    sync_equivalence_replace_once(
        project,
        "analytics/app.py",
        "def current_tenant() -> str:",
        "def new_public_helper() -> int:\n    return 0\n\n\ndef current_tenant() -> str:",
    )
}

fn patch_python_conftest_edit(project: &Path) -> Result<(), String> {
    // A `conftest.py` edit alters ancestor pytest-fixture context for its whole
    // subtree, which the module interface projection deliberately does not model,
    // so a conftest change always forces a full rebuild regardless of its
    // interface hash. This is the safety carve-out that keeps the interface fast
    // path sound.
    sync_equivalence_replace_once(
        project,
        "analytics/conftest.py",
        "return \"default\"",
        "return \"primary\"",
    )
}

fn patch_java_add(project: &Path) -> Result<(), String> {
    let path = project.join("service/java/ExtraServiceTest.java");
    fs::write(
        &path,
        "package com.example.orders;\n\nimport org.junit.jupiter.api.Test;\n\nclass ExtraServiceTest {\n    @Test\n    void reservesOrder() {\n        assert true;\n    }\n}\n",
    )
    .map_err(|_| "could not add scenario file 'service/java/ExtraServiceTest.java'".to_string())
}

fn patch_java_delete(project: &Path) -> Result<(), String> {
    fs::remove_file(project.join("service/java/OrderServiceTest.java")).map_err(|_| {
        "could not delete scenario file 'service/java/OrderServiceTest.java'".to_string()
    })
}

/// Canonical serialization with recursively sorted object keys, so the compared
/// strings are independent of the product's JSON map ordering.
fn canonical_json_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::Value::String((*key).clone()),
                        canonical_json_string(&map[*key])
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        serde_json::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonical_json_string).collect();
            format!("[{}]", inner.join(","))
        }
        other => other.to_string(),
    }
}

fn sync_equivalence_sorted_items(value: &serde_json::Value, key: &str) -> Vec<String> {
    let mut items: Vec<String> = value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|array| array.iter().map(canonical_json_string).collect())
        .unwrap_or_default();
    items.sort();
    items
}

/// One compared read surface's multiset difference between the incremental
/// state (B) and the clean rebuild (C), with bounded samples.
struct SyncEquivalenceSurfaceDiff {
    surface: &'static str,
    equal: bool,
    b_only_sample: Vec<String>,
    c_only_sample: Vec<String>,
    note: Option<String>,
}

const SYNC_EQUIVALENCE_DIFF_SAMPLE_CAP: usize = 8;

fn sync_equivalence_multiset_diff(b: &[String], c: &[String]) -> (Vec<String>, Vec<String>) {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    for item in b {
        *counts.entry(item.as_str()).or_default() += 1;
    }
    for item in c {
        *counts.entry(item.as_str()).or_default() -= 1;
    }
    let mut b_only = Vec::new();
    let mut c_only = Vec::new();
    for (item, count) in counts {
        if count > 0 {
            b_only.push(item.to_string());
        } else if count < 0 {
            c_only.push(item.to_string());
        }
    }
    b_only.truncate(SYNC_EQUIVALENCE_DIFF_SAMPLE_CAP);
    c_only.truncate(SYNC_EQUIVALENCE_DIFF_SAMPLE_CAP);
    (b_only, c_only)
}

fn sync_equivalence_surface(
    surface: &'static str,
    b: &[String],
    c: &[String],
) -> SyncEquivalenceSurfaceDiff {
    let (b_only_sample, c_only_sample) = sync_equivalence_multiset_diff(b, c);
    SyncEquivalenceSurfaceDiff {
        surface,
        equal: b_only_sample.is_empty() && c_only_sample.is_empty(),
        b_only_sample,
        c_only_sample,
        note: None,
    }
}

/// The one semantic-fact origin a worker-less clean rebuild genuinely cannot
/// reproduce: raw facts from an external TypeScript worker (`typescript`),
/// retained for unchanged files by a worker-less incremental sync — the design's
/// single sanctioned divergence. `cargo_metadata` is deliberately excluded: the
/// in-binary Rust provider runs under the identical isolated environment in both
/// the incremental base build and the clean rebuild, so its facts are
/// reproducible in C and must be compared by strict equality like any other
/// local engine.
fn sync_equivalence_is_worker_provider_origin(origin_engine: &str) -> bool {
    origin_engine == "typescript"
}

/// Canonical content tuple of a semantic fact: every persisted field except the
/// order/history-assigned `fact_id`/`evidence_id` (re-keyed across generations).
/// `content_hash` IS included — it records the file content the fact was
/// extracted from and is exactly the field that distinguishes a wrongfully
/// retained (stale) fact from a correctly re-parsed one when all other fields
/// coincide; in a correct incremental sync every retained fact's hash equals the
/// current file hash, so equality with the clean rebuild still holds.
/// `target` Option-ness is encoded explicitly (`some:`/`none`) so `None` and
/// `Some("")` never collapse, and assumptions keep their emitted order joined by
/// the unit separator so `["a,b"]` and `["a","b"]` cannot canonicalize alike.
fn sync_equivalence_fact_tuple(
    fact: &repogrammar::ports::index_store::IndexedSemanticFactRecord,
) -> String {
    let target = match &fact.target {
        Some(value) => format!("some:{value}"),
        None => "none".to_string(),
    };
    [
        fact.path.clone(),
        fact.content_hash.as_str().to_string(),
        fact.start_byte.to_string(),
        fact.end_byte.to_string(),
        fact.code_unit_id.clone(),
        fact.kind.clone(),
        fact.subject.clone(),
        target,
        fact.certainty.clone(),
        fact.origin_engine.clone(),
        fact.origin_engine_version.clone(),
        fact.origin_method.clone(),
        fact.assumptions.join("\u{1f}"),
        fact.note.clone(),
    ]
    .join("\u{1f}")
}

/// Store-port ledgers read through the `SqliteIndexStore` handle: the
/// semantic-fact multiset (partitioned local vs external-worker provider), the
/// IR graph (which has bespoke incremental copy-forward logic that a full
/// rebuild replays differently), and the repo-shape stats. These are not exposed
/// by any product CLI read surface.
struct SyncEquivalenceStoreLedgers {
    facts_local: Vec<String>,
    facts_provider: Vec<String>,
    provider_retained_ok: bool,
    ir_nodes: Vec<String>,
    ir_edges: Vec<String>,
    repo_shape: Vec<String>,
}

fn sync_equivalence_dump_store_ledgers(
    state_dir: &Path,
) -> Result<SyncEquivalenceStoreLedgers, String> {
    use std::collections::BTreeMap;
    let store = SqliteIndexStore::new(state_dir);
    let facts = store
        .list_active_semantic_facts()
        .map_err(|_| "could not read the semantic-fact ledger".to_string())?
        .facts;
    let file_hashes: BTreeMap<String, String> = store
        .list_active_indexed_files()
        .map_err(|_| "could not read the indexed-file ledger".to_string())?
        .files
        .into_iter()
        .map(|file| (file.path, file.content_hash.as_str().to_string()))
        .collect();
    let mut facts_local = Vec::new();
    let mut facts_provider = Vec::new();
    let mut provider_retained_ok = true;
    for fact in &facts {
        let tuple = sync_equivalence_fact_tuple(fact);
        if sync_equivalence_is_worker_provider_origin(&fact.origin_engine) {
            let retained = file_hashes
                .get(&fact.path)
                .is_some_and(|hash| hash == fact.content_hash.as_str());
            if !retained {
                provider_retained_ok = false;
            }
            facts_provider.push(tuple);
        } else {
            facts_local.push(tuple);
        }
    }
    facts_local.sort();
    facts_provider.sort();

    let ir = store
        .list_active_ir_graph()
        .map_err(|_| "could not read the IR graph ledger".to_string())?;
    let mut ir_nodes: Vec<String> = ir
        .nodes
        .iter()
        .map(|node| {
            [
                node.id.clone(),
                node.code_unit_id.clone(),
                node.kind.clone(),
                node.payload_json.clone(),
            ]
            .join("\u{1f}")
        })
        .collect();
    ir_nodes.sort();
    let mut ir_edges: Vec<String> = ir
        .edges
        .iter()
        .map(|edge| {
            [
                edge.from_node_id.clone(),
                edge.to_node_id.clone(),
                edge.label.clone(),
            ]
            .join("\u{1f}")
        })
        .collect();
    ir_edges.sort();

    let shape = store
        .active_repo_shape_stats()
        .map_err(|_| "could not read the repo-shape stats".to_string())?;
    let mut repo_shape: Vec<String> = shape
        .by_language
        .iter()
        .map(|language| {
            format!(
                "lang\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                language.language,
                language.indexed_file_count,
                language.indexed_code_unit_count,
                language.eligible_code_units,
                language.family_count,
                language.family_member_count,
                language.covered_code_units,
            )
        })
        .collect();
    repo_shape.push(format!(
        "totals\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        shape.indexed_file_count,
        shape.indexed_code_unit_count,
        shape.semantic_fact_count,
        shape.eligible_code_units,
        shape.family_count,
        shape.family_member_count,
        shape.covered_code_units,
    ));
    repo_shape.sort();

    Ok(SyncEquivalenceStoreLedgers {
        facts_local,
        facts_provider,
        provider_retained_ok,
        ir_nodes,
        ir_edges,
        repo_shape,
    })
}

/// A canonical, order-independent dump of one active generation across the
/// product read surfaces plus the store-port ledgers.
struct SyncEquivalenceStateDump {
    files: Vec<String>,
    units: Vec<String>,
    families: Vec<String>,
    family_details: Vec<String>,
    unknowns: Vec<String>,
    facts_local: Vec<String>,
    facts_provider: Vec<String>,
    provider_retained_ok: bool,
    ir_nodes: Vec<String>,
    ir_edges: Vec<String>,
    repo_shape: Vec<String>,
}

struct SyncEquivalenceWorkspace {
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
    tools: PathBuf,
    binary: PathBuf,
    python: PathBuf,
}

impl SyncEquivalenceWorkspace {
    fn new(binary: &Path, fixture_source: &Path, label: &str) -> Result<Self, String> {
        let root = unique_sync_equivalence_root(label);
        let home = root.join("home");
        let project = root.join("project");
        let tools = root.join("tools");
        fs::create_dir_all(&home)
            .map_err(|_| "could not create isolated sync-equivalence HOME".to_string())?;
        fs::create_dir_all(&project)
            .map_err(|_| "could not create isolated sync-equivalence project".to_string())?;
        fs::create_dir_all(&tools)
            .map_err(|_| "could not create isolated sync-equivalence tool PATH".to_string())?;
        let python = prepare_isolated_tool_path(&tools)?;
        copy_dir_sorted(fixture_source, &project)?;
        Ok(Self {
            root,
            home,
            project,
            tools,
            binary: binary.to_path_buf(),
            python,
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
            .env("REPOGRAMMAR_PYTHON_EXECUTABLE", &self.python)
            .env("REPOGRAMMAR_TELEMETRY", "0")
            .env("DO_NOT_TRACK", "1");
        command
    }

    fn project_arg(&self) -> Result<String, String> {
        self.project
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| "isolated sync-equivalence project path was not UTF-8".to_string())
    }

    fn state_dir(&self) -> PathBuf {
        self.project.join(".repogrammar")
    }

    fn run_json(&self, command: &str, extra: &[&str]) -> Result<serde_json::Value, String> {
        let project = self.project_arg()?;
        let mut args: Vec<String> = vec![command.to_string(), "--project".to_string(), project];
        args.extend(extra.iter().map(|value| (*value).to_string()));
        let output = self
            .command()
            .args(&args)
            .output()
            .map_err(|_| format!("sync-equivalence '{command}' could not execute"))?;
        if !output.status.success() {
            return Err(format!(
                "sync-equivalence '{command}' returned a failure status"
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| format!("sync-equivalence '{command}' output was not UTF-8"))?;
        serde_json::from_str(stdout.trim())
            .map_err(|_| format!("sync-equivalence '{command}' output was not JSON"))
    }

    fn init(&self) -> Result<(), String> {
        self.run_json("init", &["--yes", "--json", "--progress", "never"])?;
        Ok(())
    }

    fn resync(&self) -> Result<(), String> {
        let value = self.run_json("resync", &["--json", "--progress", "never"])?;
        if value.get("status").and_then(serde_json::Value::as_str) != Some("complete") {
            return Err("sync-equivalence 'resync' did not complete".to_string());
        }
        Ok(())
    }

    fn dump_state(&self) -> Result<SyncEquivalenceStateDump, String> {
        let files_value = self.run_json("files", &["--json"])?;
        let units_value = self.run_json("units", &["--json"])?;
        let families_value = self.run_json("families", &["--json"])?;
        let unknowns_value = self.run_json("unknowns", &["--json"])?;

        let files = sync_equivalence_sorted_items(&files_value, "files");
        let units = sync_equivalence_sorted_items(&units_value, "units");
        let mut families = sync_equivalence_sorted_items(&families_value, "families");
        for listing_unknown in sync_equivalence_sorted_items(&families_value, "unknowns") {
            families.push(format!("list_unknown|{listing_unknown}"));
        }
        families.sort();

        let mut family_ids: Vec<String> = families_value
            .get("families")
            .and_then(serde_json::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|item| {
                        item.get("family_id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        family_ids.sort();
        family_ids.dedup();
        let mut family_details = Vec::new();
        for family_id in &family_ids {
            // `--mode deep` is required: the default compact mode returns an
            // empty selected-evidence array, so the family-evidence ledger would
            // never actually be compared.
            let detail = self.run_json("family", &[family_id, "--mode", "deep", "--json"])?;
            for key in [
                "family",
                "members",
                "variation_slots",
                "evidence",
                "unknowns",
            ] {
                let value = detail.get(key).cloned().unwrap_or(serde_json::Value::Null);
                let value = if key == "evidence" {
                    sync_equivalence_strip_evidence_presentation(value)
                } else {
                    value
                };
                let canon = canonical_json_string(&value);
                family_details.push(format!("{family_id}|{key}|{canon}"));
            }
        }
        family_details.sort();

        let mut inventory = unknowns_value
            .get("unknown_inventory")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(map) = &mut inventory {
            map.remove("active_generation");
        }
        let unknowns = vec![format!("inventory|{}", canonical_json_string(&inventory))];

        let ledgers = sync_equivalence_dump_store_ledgers(&self.state_dir())?;
        Ok(SyncEquivalenceStateDump {
            files,
            units,
            families,
            family_details,
            unknowns,
            facts_local: ledgers.facts_local,
            facts_provider: ledgers.facts_provider,
            provider_retained_ok: ledgers.provider_retained_ok,
            ir_nodes: ledgers.ir_nodes,
            ir_edges: ledgers.ir_edges,
            repo_shape: ledgers.repo_shape,
        })
    }
}

/// Strips the non-semantic fields from deep-mode family-evidence rows: the
/// order/history-assigned `evidence_id` (re-keyed across generations, like
/// `fact_id`) and `estimated_tokens` (a query-presentation estimate, not
/// persisted family state). The load-bearing rows — `code_unit_id`, `path`,
/// `content_hash`, byte ranges, `covered_claims`, `note` — are retained and
/// compared.
fn sync_equivalence_strip_evidence_presentation(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(sync_equivalence_strip_evidence_presentation)
                .collect(),
        ),
        serde_json::Value::Object(mut map) => {
            map.remove("evidence_id");
            map.remove("estimated_tokens");
            serde_json::Value::Object(map)
        }
        other => other,
    }
}

impl Drop for SyncEquivalenceWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_sync_equivalence_root(label: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let sanitized: String = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    env::temp_dir().join(format!(
        "repogrammar-sync-equivalence-{}-{sanitized}-{nanos}-{sequence}",
        std::process::id()
    ))
}

struct SyncEquivalenceScenarioResult {
    id: &'static str,
    patch_summary: &'static str,
    sync_mode: String,
    fallback_reason: Option<String>,
    reparsed_files: Option<u64>,
    expected_reparsed_files: Option<usize>,
    equal: bool,
    outcome: &'static str,
    expected_outcome: &'static str,
    expected_fallback_reason: Option<&'static str>,
    pass: bool,
    surfaces: Vec<SyncEquivalenceSurfaceDiff>,
}

fn sync_equivalence_compare(
    incremental: &SyncEquivalenceStateDump,
    clean: &SyncEquivalenceStateDump,
) -> Vec<SyncEquivalenceSurfaceDiff> {
    let mut surfaces = vec![
        sync_equivalence_surface("indexed_files", &incremental.files, &clean.files),
        sync_equivalence_surface("code_units", &incremental.units, &clean.units),
        sync_equivalence_surface("ir_nodes", &incremental.ir_nodes, &clean.ir_nodes),
        sync_equivalence_surface("ir_edges", &incremental.ir_edges, &clean.ir_edges),
        sync_equivalence_surface("families", &incremental.families, &clean.families),
        sync_equivalence_surface(
            "family_details",
            &incremental.family_details,
            &clean.family_details,
        ),
        sync_equivalence_surface("unknown_inventory", &incremental.unknowns, &clean.unknowns),
        sync_equivalence_surface(
            "repo_shape_stats",
            &incremental.repo_shape,
            &clean.repo_shape,
        ),
        sync_equivalence_surface(
            "semantic_facts_local",
            &incremental.facts_local,
            &clean.facts_local,
        ),
    ];
    // External-worker (`typescript`) provider facts retained by a worker-less
    // incremental sync are the one sanctioned divergence, checked against the
    // retention rule rather than by equality with the clean rebuild. The rule is
    // two-sided: it also fails on clean-only provider facts unmatched in the
    // incremental state (a dropped-fact regression). In worker-less runs both
    // buckets are empty and the surface is trivially equal.
    let (b_only, c_only) =
        sync_equivalence_multiset_diff(&incremental.facts_provider, &clean.facts_provider);
    let provider_equal = incremental.provider_retained_ok && c_only.is_empty();
    surfaces.push(SyncEquivalenceSurfaceDiff {
        surface: "semantic_facts_provider_retained",
        equal: provider_equal,
        b_only_sample: if incremental.provider_retained_ok {
            Vec::new()
        } else {
            b_only
        },
        c_only_sample: c_only,
        note: Some(format!(
            "incremental provider facts: {}, clean provider facts: {}, retention satisfied: {}",
            incremental.facts_provider.len(),
            clean.facts_provider.len(),
            incremental.provider_retained_ok
        )),
    });
    surfaces
}

/// A scenario passes only when the observed outcome matches the declared
/// expectation and, for a fallback, the observed reason matches too. This is
/// what makes the exit-0 gate non-trivial: an unexpected EQUAL (a regressed gate
/// that took the incremental path), an unexpected fallback (a misfiring
/// preflight), a wrong fallback reason, or any INEQUAL all fail.
fn sync_equivalence_scenario_passes(
    outcome: &str,
    fallback_reason: Option<&str>,
    reparsed_files: Option<u64>,
    expected_outcome: &str,
    expected_fallback_reason: Option<&str>,
    expected_reparsed_files: Option<usize>,
) -> bool {
    let reparsed_ok = match expected_reparsed_files {
        Some(expected) => reparsed_files == Some(expected as u64),
        None => true,
    };
    outcome == expected_outcome && fallback_reason == expected_fallback_reason && reparsed_ok
}

fn run_sync_equivalence_scenario(
    binary: &Path,
    fixture_source: &Path,
    scenario: &SyncEquivalenceScenario,
) -> Result<SyncEquivalenceScenarioResult, String> {
    // Workspace A -> B: full base build, scripted patch, then incremental sync.
    let incremental_workspace =
        SyncEquivalenceWorkspace::new(binary, fixture_source, &format!("{}-inc", scenario.id))?;
    incremental_workspace.init()?;
    incremental_workspace.resync()?;
    (scenario.apply)(&incremental_workspace.project)?;
    let sync_value = incremental_workspace.run_json("sync", &["--json", "--progress", "never"])?;
    let sync_mode = sync_value
        .get("sync_mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let fallback_reason = sync_value
        .get("fallback_reason")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let reparsed_files = sync_value
        .get("reparsed_files")
        .and_then(serde_json::Value::as_u64);
    let incremental_dump = incremental_workspace.dump_state()?;

    // Workspace C: patch first, then a clean full build over the same worktree.
    let clean_workspace =
        SyncEquivalenceWorkspace::new(binary, fixture_source, &format!("{}-clean", scenario.id))?;
    (scenario.apply)(&clean_workspace.project)?;
    clean_workspace.init()?;
    clean_workspace.resync()?;
    let clean_dump = clean_workspace.dump_state()?;

    let surfaces = sync_equivalence_compare(&incremental_dump, &clean_dump);
    let equal = surfaces.iter().all(|surface| surface.equal);
    let outcome = if !equal {
        "INEQUAL"
    } else if sync_mode == "full_rebuild_fallback" {
        "FELL_BACK"
    } else {
        "EQUAL"
    };
    let pass = sync_equivalence_scenario_passes(
        outcome,
        fallback_reason.as_deref(),
        reparsed_files,
        scenario.expected_outcome,
        scenario.expected_fallback_reason,
        scenario.expected_reparsed_files,
    );
    Ok(SyncEquivalenceScenarioResult {
        id: scenario.id,
        patch_summary: scenario.patch_summary,
        sync_mode,
        fallback_reason,
        reparsed_files,
        expected_reparsed_files: scenario.expected_reparsed_files,
        equal,
        outcome,
        expected_outcome: scenario.expected_outcome,
        expected_fallback_reason: scenario.expected_fallback_reason,
        pass,
        surfaces,
    })
}

fn sync_equivalence_report(
    fixture: &str,
    binary: &Path,
    results: &[SyncEquivalenceScenarioResult],
) -> serde_json::Value {
    let scenarios: Vec<serde_json::Value> = results
        .iter()
        .map(|result| {
            let surfaces: Vec<serde_json::Value> = result
                .surfaces
                .iter()
                .map(|surface| {
                    serde_json::json!({
                        "surface": surface.surface,
                        "equal": surface.equal,
                        "b_only_sample": surface.b_only_sample,
                        "c_only_sample": surface.c_only_sample,
                        "note": surface.note,
                    })
                })
                .collect();
            serde_json::json!({
                "id": result.id,
                "patch_summary": result.patch_summary,
                "sync_mode": result.sync_mode,
                "fallback_reason": result.fallback_reason,
                "reparsed_files": result.reparsed_files,
                "expected_reparsed_files": result.expected_reparsed_files,
                "equal": result.equal,
                "outcome": result.outcome,
                "expected_outcome": result.expected_outcome,
                "expected_fallback_reason": result.expected_fallback_reason,
                "pass": result.pass,
                "surfaces": surfaces,
            })
        })
        .collect();
    serde_json::json!({
        "schema": "sync-equivalence.v1",
        "fixture": fixture,
        "binary": binary.to_string_lossy(),
        "scenario_count": results.len(),
        "all_passed": results.iter().all(|result| result.pass),
        "scenarios": scenarios,
    })
}

fn resolve_sync_equivalence_fixture(root: &Path, fixture: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(fixture);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| "fixture root is unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("fixture root must be a directory".to_string());
    }
    fs::canonicalize(path).map_err(|_| "fixture root is unavailable".to_string())
}

fn sync_equivalence_command(root: &Path, args: &[String]) -> Result<String, String> {
    let mut fixture = None;
    let mut out = None;
    let mut bin = None;
    let mut scenario: Option<String> = None;
    let mut all = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fixture" => fixture = Some(product_eval_take_value(args, &mut index, "--fixture")?),
            "--out" => out = Some(product_eval_take_value(args, &mut index, "--out")?),
            "--bin" => bin = Some(product_eval_take_value(args, &mut index, "--bin")?),
            "--scenario" => {
                scenario = Some(product_eval_take_value(args, &mut index, "--scenario")?)
            }
            "--all" => all = true,
            other => return Err(format!("unknown sync-equivalence argument '{other}'")),
        }
        index += 1;
    }
    let fixture = fixture.ok_or_else(|| "--fixture <path> is required".to_string())?;
    let out = out.ok_or_else(|| "--out <dir> is required".to_string())?;
    if all && scenario.is_some() {
        return Err("--all and --scenario are mutually exclusive".to_string());
    }

    let binary = resolve_product_binary(root, bin.as_deref())?;
    let fixture_source = resolve_sync_equivalence_fixture(root, &fixture)?;
    let selected: Vec<&SyncEquivalenceScenario> = match &scenario {
        Some(id) => vec![SYNC_EQUIVALENCE_SCENARIOS
            .iter()
            .find(|candidate| candidate.id == id)
            .ok_or_else(|| format!("unknown scenario '{id}'"))?],
        None => SYNC_EQUIVALENCE_SCENARIOS.iter().collect(),
    };

    let out_dir = if Path::new(&out).is_absolute() {
        PathBuf::from(&out)
    } else {
        root.join(&out)
    };
    fs::create_dir_all(&out_dir).map_err(|_| "could not create --out directory".to_string())?;

    let mut results = Vec::new();
    for scenario in selected {
        results.push(run_sync_equivalence_scenario(
            &binary,
            &fixture_source,
            scenario,
        )?);
    }

    let report = sync_equivalence_report(&fixture, &binary, &results);
    let report_path = out_dir.join("sync-equivalence.json");
    let serialized = serde_json::to_string_pretty(&report)
        .map_err(|_| "could not serialize the sync-equivalence report".to_string())?;
    fs::write(&report_path, format!("{serialized}\n"))
        .map_err(|_| "could not write the sync-equivalence report".to_string())?;

    let mut summary = format!(
        "sync-equivalence: {} scenario(s) over {}\n",
        results.len(),
        fixture
    );
    for result in &results {
        let fallback = result
            .fallback_reason
            .as_deref()
            .map(|reason| format!("[{reason}]"))
            .unwrap_or_default();
        summary.push_str(&format!(
            "  {:<15} {:<22} {:<10} expected {:<10} {}\n",
            result.id,
            format!("{}{}", result.sync_mode, fallback),
            result.outcome,
            result.expected_outcome,
            if result.pass { "PASS" } else { "FAIL" },
        ));
    }
    summary.push_str(&format!("report: {}\n", report_path.display()));

    let failed: Vec<&str> = results
        .iter()
        .filter(|result| !result.pass)
        .map(|result| result.id)
        .collect();
    if failed.is_empty() {
        Ok(summary)
    } else {
        Err(format!(
            "{summary}outcome mismatch in scenario(s): {}",
            failed.join(", ")
        ))
    }
}

fn product_eval_command(root: &Path, args: &[String]) -> Result<String, String> {
    let mut corpus = None;
    let mut out = None;
    let mut repetitions = 3usize;
    let mut bin = None;
    let mut condition: Option<String> = None;
    let mut baseline: Option<EvalBaseline> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => corpus = Some(product_eval_take_value(args, &mut index, "--corpus")?),
            "--out" => out = Some(product_eval_take_value(args, &mut index, "--out")?),
            "--repetitions" => {
                let raw = product_eval_take_value(args, &mut index, "--repetitions")?;
                repetitions = raw
                    .parse::<usize>()
                    .map_err(|_| "--repetitions must be a positive integer".to_string())?;
            }
            "--bin" => bin = Some(product_eval_take_value(args, &mut index, "--bin")?),
            "--condition" => {
                let raw = product_eval_take_value(args, &mut index, "--condition")?;
                condition = Some(validate_condition_token(&raw)?);
            }
            "--baseline" => {
                let raw = product_eval_take_value(args, &mut index, "--baseline")?;
                baseline = Some(EvalBaseline::from_token(&raw)?);
            }
            other => return Err(format!("unknown product-eval argument '{other}'")),
        }
        index += 1;
    }
    let corpus = corpus.ok_or_else(|| "--corpus <path> is required".to_string())?;
    let out = out.ok_or_else(|| "--out <dir> is required".to_string())?;
    let condition = resolve_eval_condition(condition, baseline)?;
    run_product_eval(
        root,
        &corpus,
        &out,
        repetitions,
        bin.as_deref(),
        &condition,
        baseline,
    )
}

fn product_eval_take_value(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .clone();
    *index += 1;
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn run_product_eval(
    root: &Path,
    corpus_path: &str,
    out_dir: &str,
    repetitions: usize,
    bin_override: Option<&str>,
    condition: &str,
    baseline: Option<EvalBaseline>,
) -> Result<String, String> {
    if repetitions == 0 {
        return Err("--repetitions must be at least 1".to_string());
    }
    let binary = resolve_product_binary(root, bin_override)?;
    let corpus_absolute = if Path::new(corpus_path).is_absolute() {
        PathBuf::from(corpus_path)
    } else {
        root.join(corpus_path)
    };
    let corpus_text = fs::read_to_string(&corpus_absolute)
        .map_err(|_| "corpus file is unavailable".to_string())?;
    let corpus_value: serde_json::Value = serde_json::from_str(&corpus_text)
        .map_err(|_| "corpus file was not valid JSON".to_string())?;
    let corpus = EvalCorpus::from_value(&corpus_value)?;

    let commit = resolve_git_commit(root, "HEAD").unwrap_or_else(|_| "unknown".to_string());
    let started_at = rfc3339_utc(unix_seconds_now());

    let mut base_workspaces: Vec<(String, EvalWorkspace)> = Vec::new();
    // Populated only in baseline mode: the product's `families --json` listing
    // (active generation + family ids) captured once per indexed fixture.
    let mut baseline_families: std::collections::BTreeMap<String, (Option<String>, Vec<String>)> =
        std::collections::BTreeMap::new();
    let mut fixture_results: Vec<serde_json::Value> = Vec::new();
    for fixture in &corpus.fixtures {
        let source = fs::canonicalize(root.join(&fixture.root))
            .map_err(|_| format!("fixture root '{}' is unavailable", fixture.root))?;
        let fixture_version = match fixture_version_hash(&source) {
            Ok(hash) => hash,
            Err(error) => {
                retain_workspaces(&base_workspaces);
                return Err(error);
            }
        };
        let workspace = match EvalWorkspace::new(&binary, &source) {
            Ok(workspace) => workspace,
            Err(error) => {
                retain_workspaces(&base_workspaces);
                return Err(error);
            }
        };
        if let Err(error) = workspace.init() {
            workspace.retain();
            retain_workspaces(&base_workspaces);
            return Err(error);
        }
        let resync = match workspace.resync() {
            Ok(resync) => resync,
            Err(error) => {
                workspace.retain();
                retain_workspaces(&base_workspaces);
                return Err(error);
            }
        };
        if baseline.is_some() {
            match workspace.families() {
                Ok(families) => {
                    baseline_families.insert(fixture.fixture_id.clone(), families);
                }
                Err(error) => {
                    workspace.retain();
                    retain_workspaces(&base_workspaces);
                    return Err(error);
                }
            }
        }
        fixture_results.push(serde_json::json!({
            "fixture_id": fixture.fixture_id,
            "fixture_version": fixture_version,
            "resync_latency_ms": resync.latency_ms,
            "discovered_files": resync.discovered_files,
            "stored_files": resync.stored_files,
        }));
        base_workspaces.push((fixture.fixture_id.clone(), workspace));
    }

    let mut query_results: Vec<serde_json::Value> = Vec::new();
    let mut table_rows: Vec<(String, String, &'static str, u128)> = Vec::new();
    let mut all_latencies: Vec<u128> = Vec::new();
    let mut matches = 0usize;
    let mut false_family_selections = 0usize;
    let mut by_kind: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    // (intent, kind, expected, actual, is_match) retained so intent-partitioned
    // metrics can be computed once after every query has been driven.
    let mut metric_inputs: Vec<(Option<EvalIntent>, String, EvalExpected, EvalActual, bool)> =
        Vec::new();

    for query in &corpus.queries {
        let outcome = match baseline {
            // Baseline mode scores the query against the fixture's pre-fetched
            // family listing; it never drives the product per query and ignores
            // source mutations, which the naive control does not model.
            Some(EvalBaseline::TokenOverlap) => {
                let (active_generation, family_ids) =
                    baseline_families.get(&query.fixture_id).ok_or_else(|| {
                        format!(
                            "baseline is missing the family listing for fixture '{}'",
                            query.fixture_id
                        )
                    })?;
                run_baseline_query(query, active_generation.as_deref(), family_ids, repetitions)
            }
            None => {
                let base = base_workspaces
                    .iter()
                    .find(|(fixture_id, _)| fixture_id == &query.fixture_id)
                    .map(|(_, workspace)| workspace)
                    .ok_or_else(|| {
                        format!(
                            "query '{}' references unindexed fixture '{}'",
                            query.query_id, query.fixture_id
                        )
                    })?;
                if let Some(mutation) = &query.mutation {
                    let fixture = corpus
                        .fixtures
                        .iter()
                        .find(|fixture| fixture.fixture_id == query.fixture_id)
                        .ok_or_else(|| {
                            "mutation query references an unknown fixture".to_string()
                        })?;
                    let source = fs::canonicalize(root.join(&fixture.root))
                        .map_err(|_| "fixture root is unavailable".to_string())?;
                    let mutation_workspace = match EvalWorkspace::new(&binary, &source) {
                        Ok(workspace) => workspace,
                        Err(error) => {
                            retain_workspaces(&base_workspaces);
                            return Err(error);
                        }
                    };
                    let run = (|| {
                        mutation_workspace.init()?;
                        mutation_workspace.resync()?;
                        mutation_workspace.apply_mutation(mutation)?;
                        mutation_workspace.run_query(query, repetitions)
                    })();
                    match run {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            mutation_workspace.retain();
                            retain_workspaces(&base_workspaces);
                            return Err(error);
                        }
                    }
                } else {
                    match base.run_query(query, repetitions) {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            retain_workspaces(&base_workspaces);
                            return Err(error);
                        }
                    }
                }
            }
        };
        let (actual, latencies) = outcome;

        let (is_match, mismatch_fields) = evaluate_match(&query.expected, &actual);
        if is_match {
            matches += 1;
        }
        if is_false_family_selection(&query.expected, &actual) {
            false_family_selections += 1;
        }
        let counters = by_kind.entry(query.kind.clone()).or_insert((0, 0));
        counters.0 += 1;
        if is_match {
            counters.1 += 1;
        }
        all_latencies.extend(latencies.iter().copied());
        let query_p50 = percentile(&latencies, 50);
        table_rows.push((
            query.query_id.clone(),
            query.kind.clone(),
            if is_match { "match" } else { "mismatch" },
            query_p50,
        ));
        query_results.push(serde_json::json!({
            "query_id": query.query_id,
            "fixture_id": query.fixture_id,
            "kind": query.kind,
            "intent": query.intent.map(EvalIntent::as_str),
            "operation": query.operation.as_str(),
            "target": query.target,
            "expected": query.expected.to_value(),
            "actual": actual.to_value(),
            "match": is_match,
            "mismatch_fields": mismatch_fields,
            "reciprocal_rank": if query.intent == Some(EvalIntent::Retrieval) {
                serde_json::json!(reciprocal_rank(&query.expected, &actual))
            } else {
                serde_json::Value::Null
            },
            "latency_ms_all_reps": latencies,
            "latency_ms_p50": query_p50,
        }));
        metric_inputs.push((
            query.intent,
            query.kind.clone(),
            query.expected.clone(),
            actual,
            is_match,
        ));
    }

    let finished_at = rfc3339_utc(unix_seconds_now());
    let total = corpus.queries.len();
    let mismatches = total - matches;
    let by_kind_value: serde_json::Map<String, serde_json::Value> = by_kind
        .iter()
        .map(|(kind, (kind_total, kind_matches))| {
            (
                kind.clone(),
                serde_json::json!({ "total": kind_total, "matches": kind_matches }),
            )
        })
        .collect();
    let metric_records: Vec<EvalMetricRecord> = metric_inputs
        .iter()
        .map(
            |(intent, kind, expected, actual, is_match)| EvalMetricRecord {
                intent: *intent,
                kind: kind.as_str(),
                expected,
                actual,
                is_match: *is_match,
            },
        )
        .collect();
    let metrics = compute_product_eval_metrics(&metric_records);
    let by_intent_value: serde_json::Map<String, serde_json::Value> = metrics
        .by_intent
        .iter()
        .map(|(intent, (intent_total, intent_matches))| {
            (
                (*intent).to_string(),
                serde_json::json!({ "total": intent_total, "matches": intent_matches }),
            )
        })
        .collect();
    let summary = serde_json::json!({
        "total": total,
        "matches": matches,
        "mismatches": mismatches,
        "by_kind": serde_json::Value::Object(by_kind_value),
        "by_intent": serde_json::Value::Object(by_intent_value),
        "latency_ms_p50": percentile(&all_latencies, 50),
        "latency_ms_p95": percentile(&all_latencies, 95),
        "false_family_selections": false_family_selections,
        "selected_on_abstention_gold": metrics.selected_on_abstention_gold,
        "metrics": metrics.to_value(),
    });

    let results = serde_json::json!({
        "schema_version": PRODUCT_EVAL_RESULTS_SCHEMA,
        "condition": condition,
        "baseline": baseline.map(EvalBaseline::as_str),
        "repogrammar_commit": commit,
        "platform": { "os": env::consts::OS, "arch": env::consts::ARCH },
        "corpus_schema_version": PRODUCT_EVAL_CORPUS_SCHEMA,
        "corpus_path": corpus_relative_path(root, &corpus_absolute, corpus_path),
        "repetitions": repetitions,
        "started_at": started_at,
        "finished_at": finished_at,
        "fixtures": fixture_results,
        "results": query_results,
        "summary": summary,
    });

    let out_dir_absolute = if Path::new(out_dir).is_absolute() {
        PathBuf::from(out_dir)
    } else {
        root.join(out_dir)
    };
    fs::create_dir_all(&out_dir_absolute)
        .map_err(|_| "could not create output directory".to_string())?;
    let out_file = out_dir_absolute.join("product-eval-results.json");
    let serialized = serde_json::to_string_pretty(&results)
        .map_err(|_| "could not serialize product-eval results".to_string())?;
    fs::write(&out_file, format!("{serialized}\n"))
        .map_err(|_| "could not write product-eval results file".to_string())?;

    let mut report = String::new();
    report.push_str(&format!(
        "product-eval corpus {} at commit {} ({} repetitions, condition {})\n",
        corpus_relative_path(root, &corpus_absolute, corpus_path),
        commit,
        repetitions,
        condition,
    ));
    report.push_str(&format!(
        "{:<34}{:<22}{:<10}{:>8}\n",
        "query_id", "kind", "verdict", "p50_ms"
    ));
    for (query_id, kind, verdict, query_p50) in &table_rows {
        report.push_str(&format!(
            "{query_id:<34}{kind:<22}{verdict:<10}{query_p50:>8}\n"
        ));
    }
    report.push_str(&format!(
        "summary: {total} queries, {matches} match, {mismatches} mismatch | p50 {}ms p95 {}ms | false_family_selections {false_family_selections} | selected_on_abstention_gold {}\n",
        percentile(&all_latencies, 50),
        percentile(&all_latencies, 95),
        metrics.selected_on_abstention_gold,
    ));
    report.push_str(&format!(
        "metrics: hit@1 {}/{} | candidate_recall {}/{} | mrr {} | correct_abstention {}/{} | false_family_rate {}/{} | unsupported_rejection {}/{} | ambiguity_precision {}/{}\n",
        metrics.hit_at_1_num,
        metrics.hit_at_1_den,
        metrics.candidate_recall_num,
        metrics.candidate_recall_den,
        format_mean(metrics.mrr_sum, metrics.mrr_den),
        metrics.correct_abstention_num,
        metrics.correct_abstention_den,
        metrics.false_family_selections,
        metrics.family_constrained_total,
        metrics.unsupported_rejection_num,
        metrics.unsupported_rejection_den,
        metrics.ambiguity_precision_num,
        metrics.ambiguity_precision_den,
    ));
    let intent_summary: Vec<String> = metrics
        .by_intent
        .iter()
        .map(|(intent, (intent_total, intent_matches))| {
            format!("{intent} {intent_matches}/{intent_total}")
        })
        .collect();
    report.push_str(&format!("by_intent: {}\n", intent_summary.join(" | ")));
    Ok(report)
}

// ===========================================================================
// S10: response payload byte-measurement harness (`payload-measure`).
//
// Serializes a fixed query corpus against one deterministic fixture index and
// records the exact response byte count and top-level field-group attribution
// per operation x category x tier (mode x verbosity). The output is a stable,
// sorted, timestamp-free `payload-bytes.summary.json` (plus a human
// `payload-bytes.md`) so a run before a precision slice and a run after it diff
// cleanly. This harness only measures; it never asserts a savings figure. A
// savings claim is declarable only from a before/after diff of two summaries.
// ===========================================================================

const PAYLOAD_MEASURE_SCHEMA: &str = "payload-bytes.v1";
const PAYLOAD_MEASURE_FIXTURE_ID: &str = "payload-measure";
const PAYLOAD_MEASURE_FIXTURE_DEFAULT: &str = "src/fixtures/evaluation/payload-measure";
const PAYLOAD_MEASURE_MODES: &[&str] = &["compact", "deep"];
const PAYLOAD_MEASURE_VERBOSITIES: &[&str] = &["minimal", "standard", "full"];

/// One query-serializer case in the fixed payload corpus. Every case is driven
/// at the full `mode x verbosity` cross product so a before/after run can
/// attribute field-group byte deltas per report variant. Targets are chosen to
/// exercise every reachable report shape on the committed fixture: Found (big/
/// small/NL/TypeScript), abstention UNKNOWN, PARTIAL_CONTEXT, exact family
/// hydration, and static-alignment conformance.
///
/// When `measure_source_spans` is set, the case is additionally driven at
/// `--mode deep --include-source-spans` (one extra row per verbosity) so the
/// `read_plan` <-> `source_spans` overlap region (the S6 dedup target, the plan's
/// largest single per-response item) is measurable — it is invisible unless
/// source spans are explicitly requested.
struct PayloadMeasureCase {
    operation: &'static str,
    category: &'static str,
    target: &'static str,
    measure_source_spans: bool,
}

const PAYLOAD_MEASURE_CASES: &[PayloadMeasureCase] = &[
    PayloadMeasureCase {
        operation: "find",
        category: "found_big_family_path",
        target: "api/routes.py",
        measure_source_spans: true,
    },
    PayloadMeasureCase {
        operation: "find",
        category: "found_small_family_path",
        target: "db/repository.py",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "find",
        category: "found_big_family_nl",
        target: "fastapi route handler",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "find",
        category: "found_typescript_family_path",
        target: "web/router.ts",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "find",
        category: "abstain_unknown_nl",
        target: "http endpoint route",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "find",
        category: "abstain_no_candidate_nl",
        target: "zzz nonexistent qqq",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "find",
        category: "partial_context_path",
        target: "legacy_app.py",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "family",
        category: "family_big",
        target: "family:python:fastapi_route:framework_fastapi_route",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "family",
        category: "family_small",
        target: "family:python:pydantic_model:framework_pydantic_model",
        measure_source_spans: false,
    },
    PayloadMeasureCase {
        operation: "check",
        category: "check_big_family_path",
        target: "api/routes.py",
        measure_source_spans: true,
    },
];

/// Number of extra `--include-source-spans` rows the corpus emits: one per
/// verbosity for each case flagged `measure_source_spans`. Used by the smoke
/// test to derive the expected row count from the corpus definition.
#[cfg(test)]
const fn payload_measure_source_span_rows() -> usize {
    let mut spans_cases = 0;
    let mut index = 0;
    while index < PAYLOAD_MEASURE_CASES.len() {
        if PAYLOAD_MEASURE_CASES[index].measure_source_spans {
            spans_cases += 1;
        }
        index += 1;
    }
    spans_cases * PAYLOAD_MEASURE_VERBOSITIES.len()
}

fn payload_measure_command(root: &Path, args: &[String]) -> Result<String, String> {
    let mut out = None;
    let mut bin = None;
    let mut fixture = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => out = Some(product_eval_take_value(args, &mut index, "--out")?),
            "--bin" => bin = Some(product_eval_take_value(args, &mut index, "--bin")?),
            "--fixture" => fixture = Some(product_eval_take_value(args, &mut index, "--fixture")?),
            other => return Err(format!("unknown payload-measure argument '{other}'")),
        }
        index += 1;
    }
    let out = out.ok_or_else(|| "--out <dir> is required".to_string())?;
    run_payload_measure(root, &out, bin.as_deref(), fixture.as_deref())
}

/// Attributes serialized compact bytes to each top-level field of a response,
/// mirroring the audit methodology (`len(json.dumps(value, separators=...))`).
/// The result is a sorted map, so the same payload always attributes the same
/// bytes to the same fields in the same order.
fn attribute_field_bytes(value: &serde_json::Value) -> std::collections::BTreeMap<String, u64> {
    let mut attribution = std::collections::BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (field, body) in object {
            let bytes = serde_json::to_string(body)
                .map(|text| text.len() as u64)
                .unwrap_or(0);
            attribution.insert(field.clone(), bytes);
        }
    }
    attribution
}

/// Builds one measurement row plus its field-group attribution (for aggregate
/// totals). The row is a fully deterministic object; `route` is `null` when the
/// payload carries no `query_route`.
#[allow(clippy::too_many_arguments)]
fn payload_measure_row(
    operation: &str,
    category: &str,
    mode: &str,
    verbosity: &str,
    source_spans: &str,
    surface: &str,
    value: &serde_json::Value,
    total_bytes: usize,
) -> (serde_json::Value, Vec<(String, u64)>) {
    let field_bytes = attribute_field_bytes(value);
    let field_bytes_object: serde_json::Map<String, serde_json::Value> = field_bytes
        .iter()
        .map(|(field, bytes)| (field.clone(), serde_json::json!(bytes)))
        .collect();
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let route = value
        .get("query_route")
        .and_then(|route| route.get("route"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let row = serde_json::json!({
        "operation": operation,
        "category": category,
        "mode": mode,
        "verbosity": verbosity,
        "source_spans": source_spans,
        "surface": surface,
        "status": status,
        "route": route,
        "total_bytes": total_bytes,
        "field_bytes": serde_json::Value::Object(field_bytes_object),
    });
    (row, field_bytes.into_iter().collect())
}

#[allow(clippy::type_complexity)]
fn payload_measure_row_lines(
    summary: &serde_json::Value,
) -> Vec<(String, String, String, String, String, String, u64)> {
    summary
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        row["operation"].as_str().unwrap_or("").to_string(),
                        row["category"].as_str().unwrap_or("").to_string(),
                        row["mode"].as_str().unwrap_or("").to_string(),
                        row["verbosity"].as_str().unwrap_or("").to_string(),
                        row["source_spans"].as_str().unwrap_or("").to_string(),
                        row["status"].as_str().unwrap_or("").to_string(),
                        row["total_bytes"].as_u64().unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn payload_measure_top_field_groups(
    summary: &serde_json::Value,
    limit: usize,
) -> Vec<(String, u64)> {
    let mut pairs: Vec<(String, u64)> = summary
        .get("field_group_totals")
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(field, bytes)| (field.clone(), bytes.as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    pairs.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    pairs.truncate(limit);
    pairs
}

fn payload_measure_markdown(summary: &serde_json::Value) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Payload byte measurement\n\n");
    markdown.push_str(&format!(
        "- Schema: `{}`\n",
        summary["schema_version"].as_str().unwrap_or("?")
    ));
    markdown.push_str(&format!(
        "- Fixture: `{}` (version `{}`)\n",
        summary["fixture_id"].as_str().unwrap_or("?"),
        summary["fixture_version"].as_str().unwrap_or("?")
    ));
    markdown.push_str(&format!(
        "- Product schema: `{}`\n",
        summary["product_schema_version"].as_str().unwrap_or("?")
    ));
    markdown.push_str(&format!(
        "- Commit: `{}`\n",
        summary["repogrammar_commit"].as_str().unwrap_or("?")
    ));
    markdown.push_str(&format!(
        "- Rows: {} | Grand total: {} B\n\n",
        summary["totals"]["row_count"].as_u64().unwrap_or(0),
        summary["totals"]["grand_total_bytes"].as_u64().unwrap_or(0)
    ));
    markdown.push_str(
        "| operation | category | mode | verbosity | source_spans | status | total_bytes |\n",
    );
    markdown.push_str("|---|---|---|---|---|---|---|\n");
    for (operation, category, mode, verbosity, source_spans, status, total_bytes) in
        payload_measure_row_lines(summary)
    {
        markdown.push_str(&format!(
            "| {operation} | {category} | {mode} | {verbosity} | {source_spans} | {status} | {total_bytes} |\n"
        ));
    }
    markdown.push_str("\n## Heaviest field groups (summed across rows)\n\n");
    markdown.push_str("| field | bytes |\n|---|---|\n");
    for (field, bytes) in payload_measure_top_field_groups(summary, 12) {
        markdown.push_str(&format!("| {field} | {bytes} |\n"));
    }
    markdown.push_str(
        "\nA \"we saved X bytes\" claim is declarable only from a before/after diff of two \
`payload-bytes.summary.json` runs (same fixture, one with the slice, one without).\n",
    );
    markdown
}

fn payload_measure_report(summary: &serde_json::Value, summary_file: &Path) -> String {
    let mut report = String::new();
    report.push_str(&format!(
        "payload-measure fixture {} (version {}) at commit {}\n",
        summary["fixture_id"].as_str().unwrap_or("?"),
        summary["fixture_version"].as_str().unwrap_or("?"),
        summary["repogrammar_commit"].as_str().unwrap_or("?"),
    ));
    report.push_str(&format!(
        "schema {} | product schema {} | {} rows | grand total {} B\n",
        summary["schema_version"].as_str().unwrap_or("?"),
        summary["product_schema_version"].as_str().unwrap_or("?"),
        summary["totals"]["row_count"].as_u64().unwrap_or(0),
        summary["totals"]["grand_total_bytes"].as_u64().unwrap_or(0),
    ));
    report.push_str(&format!(
        "{:<18}{:<32}{:<9}{:<11}{:<7}{:<22}{:>12}\n",
        "operation", "category", "mode", "verbosity", "spans", "status", "total_bytes"
    ));
    for (operation, category, mode, verbosity, source_spans, status, total_bytes) in
        payload_measure_row_lines(summary)
    {
        report.push_str(&format!(
            "{operation:<18}{category:<32}{mode:<9}{verbosity:<11}{source_spans:<7}{status:<22}{total_bytes:>12}\n"
        ));
    }
    let top: Vec<String> = payload_measure_top_field_groups(summary, 8)
        .into_iter()
        .map(|(field, bytes)| format!("{field}={bytes}"))
        .collect();
    report.push_str(&format!("top field groups (summed): {}\n", top.join(", ")));
    report.push_str(&format!("wrote {}\n", summary_file.display()));
    report.push_str(
        "before/after: rerun payload-measure after a precision slice lands and diff \
payload-bytes.summary.json; any savings claim must cite the two-run byte table.\n",
    );
    report
}

fn run_payload_measure(
    root: &Path,
    out_dir: &str,
    bin_override: Option<&str>,
    fixture_override: Option<&str>,
) -> Result<String, String> {
    let binary = resolve_product_binary(root, bin_override)?;
    let fixture_rel = fixture_override.unwrap_or(PAYLOAD_MEASURE_FIXTURE_DEFAULT);
    let source = fs::canonicalize(root.join(fixture_rel))
        .map_err(|_| format!("payload-measure fixture '{fixture_rel}' is unavailable"))?;
    let fixture_version = fixture_version_hash(&source)?;
    let commit = resolve_git_commit(root, "HEAD").unwrap_or_else(|_| "unknown".to_string());

    let workspace = EvalWorkspace::new(&binary, &source)?;
    if let Err(error) = workspace.init() {
        workspace.retain();
        return Err(error);
    }
    if let Err(error) = workspace.resync() {
        workspace.retain();
        return Err(error);
    }

    let mut sortable: Vec<(String, serde_json::Value)> = Vec::new();
    let mut field_group_totals: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();
    let mut grand_total_bytes: u64 = 0;
    let mut product_schema_version: Option<String> = None;
    let mut fixture_shape: Option<serde_json::Value> = None;

    for case in PAYLOAD_MEASURE_CASES {
        // Base cross product: `mode x verbosity`, no rendered source spans.
        for mode in PAYLOAD_MEASURE_MODES {
            for verbosity in PAYLOAD_MEASURE_VERBOSITIES {
                let (value, total_bytes) = match workspace.capture_query_json(
                    case.operation,
                    case.target,
                    mode,
                    verbosity,
                    false,
                ) {
                    Ok(pair) => pair,
                    Err(error) => {
                        workspace.retain();
                        return Err(error);
                    }
                };
                if product_schema_version.is_none() {
                    product_schema_version = value
                        .get("schema_version")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                }
                // Record the big family's shape once so a before/after run and
                // the smoke test can detect fixture drift (member cap / count).
                if case.category == "found_big_family_path" && fixture_shape.is_none() {
                    fixture_shape = Some(serde_json::json!({
                        "category": case.category,
                        "member_count": value.get("member_count").and_then(serde_json::Value::as_u64),
                        "members_rendered": value
                            .get("members")
                            .and_then(serde_json::Value::as_array)
                            .map(|members| members.len()),
                        "members_truncated": value
                            .get("members_truncated")
                            .and_then(serde_json::Value::as_bool),
                    }));
                }
                let (row, field_bytes) = payload_measure_row(
                    case.operation,
                    case.category,
                    mode,
                    verbosity,
                    "off",
                    "cli_json",
                    &value,
                    total_bytes,
                );
                for (field, bytes) in field_bytes {
                    *field_group_totals.entry(field).or_insert(0) += bytes;
                }
                grand_total_bytes += total_bytes as u64;
                let key = format!(
                    "{}|{}|{}|{}|off",
                    case.operation, case.category, mode, verbosity
                );
                sortable.push((key, row));
            }
        }
        // Source-spans variant: deep mode with rendered spans, so the
        // `read_plan` <-> `source_spans` overlap (S6, the plan's largest single
        // per-response item) is measurable — invisible without this request.
        if case.measure_source_spans {
            for verbosity in PAYLOAD_MEASURE_VERBOSITIES {
                let (value, total_bytes) = match workspace.capture_query_json(
                    case.operation,
                    case.target,
                    "deep",
                    verbosity,
                    true,
                ) {
                    Ok(pair) => pair,
                    Err(error) => {
                        workspace.retain();
                        return Err(error);
                    }
                };
                let (row, field_bytes) = payload_measure_row(
                    case.operation,
                    case.category,
                    "deep",
                    verbosity,
                    "on",
                    "cli_json",
                    &value,
                    total_bytes,
                );
                for (field, bytes) in field_bytes {
                    *field_group_totals.entry(field).or_insert(0) += bytes;
                }
                grand_total_bytes += total_bytes as u64;
                let key = format!("{}|{}|deep|{}|on", case.operation, case.category, verbosity);
                sortable.push((key, row));
            }
        }
    }

    // Readiness: the bounded, source-free MCP `inspect_readiness` surface. The
    // CLI `status` lifecycle command is deliberately not measured here — its
    // storage internals are volatile and out of scope for the precision policy.
    let (readiness_value, readiness_bytes) = match workspace.capture_inspect_readiness() {
        Ok(pair) => pair,
        Err(error) => {
            workspace.retain();
            return Err(error);
        }
    };
    let (readiness_row, readiness_field_bytes) = payload_measure_row(
        "inspect_readiness",
        "readiness",
        "-",
        "-",
        "-",
        "mcp_tool",
        &readiness_value,
        readiness_bytes,
    );
    for (field, bytes) in readiness_field_bytes {
        *field_group_totals.entry(field).or_insert(0) += bytes;
    }
    grand_total_bytes += readiness_bytes as u64;
    sortable.push((
        "inspect_readiness|readiness|-|-|-".to_string(),
        readiness_row,
    ));

    sortable.sort_by(|left, right| left.0.cmp(&right.0));
    let rows: Vec<serde_json::Value> = sortable.into_iter().map(|(_, row)| row).collect();
    let row_count = rows.len();

    let field_group_totals_object: serde_json::Map<String, serde_json::Value> = field_group_totals
        .iter()
        .map(|(field, bytes)| (field.clone(), serde_json::json!(bytes)))
        .collect();

    let summary = serde_json::json!({
        "schema_version": PAYLOAD_MEASURE_SCHEMA,
        "fixture_id": PAYLOAD_MEASURE_FIXTURE_ID,
        "fixture_relpath": fixture_rel.replace('\\', "/"),
        "fixture_version": fixture_version,
        "product_schema_version": product_schema_version,
        "repogrammar_commit": commit,
        "mode_axis": PAYLOAD_MEASURE_MODES,
        "verbosity_axis": PAYLOAD_MEASURE_VERBOSITIES,
        "fixture_shape": fixture_shape,
        "totals": {
            "row_count": row_count,
            "grand_total_bytes": grand_total_bytes,
        },
        "field_group_totals": serde_json::Value::Object(field_group_totals_object),
        "rows": rows,
    });

    let out_dir_absolute = if Path::new(out_dir).is_absolute() {
        PathBuf::from(out_dir)
    } else {
        root.join(out_dir)
    };
    fs::create_dir_all(&out_dir_absolute)
        .map_err(|_| "could not create payload-measure output directory".to_string())?;
    let summary_file = out_dir_absolute.join("payload-bytes.summary.json");
    let serialized = serde_json::to_string_pretty(&summary)
        .map_err(|_| "could not serialize payload-measure summary".to_string())?;
    fs::write(&summary_file, format!("{serialized}\n"))
        .map_err(|_| "could not write payload-measure summary".to_string())?;
    let human_file = out_dir_absolute.join("payload-bytes.md");
    fs::write(&human_file, payload_measure_markdown(&summary))
        .map_err(|_| "could not write payload-measure human summary".to_string())?;

    Ok(payload_measure_report(&summary, &summary_file))
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
        "if: github.ref != 'refs/heads/main'",
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
    let npx_launcher_uses_isolated_workdirs =
        stable_finalizer_has_isolated_npx_workdirs(&finalizer);
    if finalizer.contains("npm publish")
        || finalizer.contains("npm stage publish")
        || finalizer.contains("npm stage approve")
        || finalizer.contains("npm stage reject")
        || finalizer.contains("npm dist-tag add")
        || finalizer.contains("npm dist-tag rm")
        || finalizer.contains("id-token: write")
        || !pack_verification_precedes_execution
        || !npx_launcher_uses_isolated_workdirs
        || stable_finalize_markers
            .iter()
            .any(|marker| !finalizer.contains(marker))
    {
        violations.push(GuardViolation::new(
            ".github/workflows/stable-release-finalize.yml",
            "StableReleaseFinalizeContract",
            "stable finalization must be manually dispatched, read-only, and verify immutable GitHub assets plus npm integrity, provenance, tags, and checkout-independent public smoke paths",
        ));
    }
}

fn stable_finalizer_has_isolated_npx_workdirs(workflow: &str) -> bool {
    const EXTERNAL_SMOKE_ROOT: &str = r#"smoke_root="${RUNNER_TEMP}/public-release-smoke""#;
    const REQUIRED_TOOL_LOOP: &str =
        "for tool in node npm npx python3 sh bash tar gzip git uname ldd getconf sha256sum curl chmod mkdir cp mv rm ln; do";
    const REQUIRED_WORKDIR_LINES: [&str; 3] = [
        r#""${smoke_root}/pinned/work" \"#,
        r#""${smoke_root}/latest/work" \"#,
        r#""${smoke_root}/preview/work""#,
    ];
    const EXACT_RUN_NPX: [&str; 17] = [
        "run_npx() {",
        r#"local lane="$1""#,
        "shift",
        "(",
        r#"cd "${smoke_root}/${lane}/work""#,
        r#"HOME="${smoke_root}/${lane}/home" \"#,
        r#"USERPROFILE="${smoke_root}/${lane}/home" \"#,
        r#"XDG_CONFIG_HOME="${smoke_root}/${lane}/home/.config" \"#,
        r#"XDG_DATA_HOME="${smoke_root}/${lane}/home/.local/share" \"#,
        r#"XDG_CACHE_HOME="${smoke_root}/${lane}/home/.cache" \"#,
        r#"CODEX_HOME="${smoke_root}/${lane}/home/.codex" \"#,
        r#"npm_config_cache="${smoke_root}/${lane}/npm-cache" \"#,
        r#"REPOGRAMMAR_NPM_CACHE_DIR="${smoke_root}/${lane}/binary-cache" \"#,
        r#"PATH="${tool_bin}" \"#,
        r#"npx --yes "$@""#,
        ")",
        "}",
    ];

    let lines = workflow.lines().map(str::trim).collect::<Vec<_>>();
    lines.iter().any(|line| line == &EXTERNAL_SMOKE_ROOT)
        && lines.iter().any(|line| line == &REQUIRED_TOOL_LOOP)
        && REQUIRED_WORKDIR_LINES
            .iter()
            .all(|required| lines.iter().any(|line| line == required))
        && lines
            .windows(EXACT_RUN_NPX.len())
            .any(|window| window == EXACT_RUN_NPX)
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

    #[test]
    fn stable_release_finalizer_requires_checkout_independent_npx_workdirs() {
        let root = TempRoot::new("stable-finalizer-npx-workdir");
        write_valid_release_contract(root.path());

        let mut violations = Vec::new();
        check_release_workflow_contract(root.path(), &mut violations);
        assert!(violations.is_empty(), "{violations:?}");

        let isolated_cd = r#"cd "${smoke_root}/${lane}/work""#;
        let external_root = r#"smoke_root="${RUNNER_TEMP}/public-release-smoke""#;
        let main_dispatch = "if: github.ref != 'refs/heads/main'";
        let tool_loop = "for tool in node npm npx python3 sh bash tar gzip git uname ldd getconf sha256sum curl chmod mkdir cp mv rm ln; do";
        for invalid in [
            valid_stable_finalize_workflow().replace(isolated_cd, "removed"),
            valid_stable_finalize_workflow()
                .replace(isolated_cd, r#"cd "${smoke_root}/${lane}/project""#),
            valid_stable_finalize_workflow().replace(
                external_root,
                r#"smoke_root="${GITHUB_WORKSPACE}/public-release-smoke""#,
            ),
            valid_stable_finalize_workflow().replace(main_dispatch, "removed"),
            valid_stable_finalize_workflow().replace(tool_loop, &tool_loop.replace(" git", "")),
        ] {
            assert_ne!(invalid, valid_stable_finalize_workflow());
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
                "{violations:?}"
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
if: github.ref != 'refs/heads/main'
smoke_root="${RUNNER_TEMP}/public-release-smoke"
for tool in node npm npx python3 sh bash tar gzip git uname ldd getconf sha256sum curl chmod mkdir cp mv rm ln; do
done
mkdir -p \
  "${smoke_root}/pinned/work" \
  "${smoke_root}/latest/work" \
  "${smoke_root}/preview/work"
run_npx() {
  local lane="$1"
  shift
  (
    cd "${smoke_root}/${lane}/work"
    HOME="${smoke_root}/${lane}/home" \
    USERPROFILE="${smoke_root}/${lane}/home" \
    XDG_CONFIG_HOME="${smoke_root}/${lane}/home/.config" \
    XDG_DATA_HOME="${smoke_root}/${lane}/home/.local/share" \
    XDG_CACHE_HOME="${smoke_root}/${lane}/home/.cache" \
    CODEX_HOME="${smoke_root}/${lane}/home/.codex" \
    npm_config_cache="${smoke_root}/${lane}/npm-cache" \
    REPOGRAMMAR_NPM_CACHE_DIR="${smoke_root}/${lane}/binary-cache" \
    PATH="${tool_bin}" \
    npx --yes "$@"
  )
}
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

    #[test]
    fn product_eval_corpus_parses_minimal_document() {
        let value = serde_json::json!({
            "schema_version": "product-eval-corpus.v1",
            "fixtures": [{"fixture_id": "f1", "root": "src/fixtures/x", "description": "d"}],
            "queries": [{
                "query_id": "q1",
                "fixture_id": "f1",
                "kind": "exact_family_id",
                "operation": "family",
                "target": "family:python:fastapi_route:framework_fastapi_route",
                "mode": "compact",
                "expected": {
                    "outcome": "ok",
                    "family": "family:python:fastapi_route:framework_fastapi_route"
                }
            }]
        });
        let corpus = EvalCorpus::from_value(&value).expect("corpus parses");
        assert_eq!(corpus.fixtures.len(), 1);
        assert_eq!(corpus.queries.len(), 1);
        assert_eq!(corpus.queries[0].operation, EvalOperation::Family);
        assert_eq!(corpus.queries[0].expected.outcome, Some(EvalOutcome::Ok));
        assert!(corpus.queries[0].mutation.is_none());
    }

    #[test]
    fn product_eval_corpus_rejects_unknown_schema_version() {
        let value = serde_json::json!({
            "schema_version": "product-eval-corpus.v2",
            "fixtures": [],
            "queries": []
        });
        assert!(EvalCorpus::from_value(&value).is_err());
    }

    #[test]
    fn product_eval_corpus_rejects_query_for_unknown_fixture() {
        let value = serde_json::json!({
            "schema_version": "product-eval-corpus.v1",
            "fixtures": [{"fixture_id": "f1", "root": "r", "description": "d"}],
            "queries": [{
                "query_id": "q1",
                "fixture_id": "missing",
                "kind": "exact_path",
                "operation": "find",
                "target": "app.py",
                "mode": "compact",
                "expected": {"outcome": "ok"}
            }]
        });
        assert!(EvalCorpus::from_value(&value).is_err());
    }

    #[test]
    fn product_eval_status_classifier_is_deterministic() {
        assert_eq!(EvalOutcome::classify_status("ok"), EvalOutcome::Ok);
        assert_eq!(
            EvalOutcome::classify_status("CONTEXT_ONLY"),
            EvalOutcome::Ok
        );
        assert_eq!(
            EvalOutcome::classify_status("PARTIAL_CONTEXT"),
            EvalOutcome::PartialContext
        );
        assert_eq!(
            EvalOutcome::classify_status("UNKNOWN"),
            EvalOutcome::Unknown
        );
        assert_eq!(
            EvalOutcome::classify_status("SOMETHING_ELSE"),
            EvalOutcome::Fallback
        );
        // Static-alignment certificates classify honestly: a committed certificate
        // (aligned or a definite deviation) is `ok`, a partial alignment is partial
        // context, and an abstaining certificate is unknown — never fallback, so a
        // committed retrieval-intent check query keeps its MRR credit.
        assert_eq!(
            EvalOutcome::classify_status("STATICALLY_ALIGNED"),
            EvalOutcome::Ok
        );
        assert_eq!(
            EvalOutcome::classify_status("STATIC_DEVIATION"),
            EvalOutcome::Ok
        );
        assert_eq!(
            EvalOutcome::classify_status("PARTIAL_ALIGNMENT"),
            EvalOutcome::PartialContext
        );
        assert_eq!(
            EvalOutcome::classify_status("INSUFFICIENT_EVIDENCE"),
            EvalOutcome::Unknown
        );
    }

    #[test]
    fn product_eval_alignment_status_is_a_first_class_matcher() {
        // A declared alignment_status gold must be enforced: a mismatch is a query
        // mismatch, and the actual reads the certificate's alignment_status field.
        let value = serde_json::json!({
            "status": "STATIC_DEVIATION",
            "alignment_status": "STATIC_DEVIATION",
            "query_route": {"selected_family_id": "family:python:fastapi_route"},
        });
        let actual = EvalActual::from_query_json(&value).expect("parses");
        assert_eq!(actual.alignment_status.as_deref(), Some("STATIC_DEVIATION"));

        let aligned_gold = EvalExpected {
            alignment_status: Some("STATICALLY_ALIGNED".to_string()),
            ..EvalExpected::default()
        };
        let (matched, mismatches) = evaluate_match(&aligned_gold, &actual);
        assert!(!matched);
        assert!(mismatches.contains(&"alignment_status".to_string()));

        let deviation_gold = EvalExpected {
            alignment_status: Some("STATIC_DEVIATION".to_string()),
            ..EvalExpected::default()
        };
        let (matched, _) = evaluate_match(&deviation_gold, &actual);
        assert!(matched);
    }

    fn sample_found_actual() -> EvalActual {
        EvalActual {
            outcome: EvalOutcome::Ok,
            route: Some("discover_hydrate_compose".to_string()),
            selected_family: Some(
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ),
            candidate_family_count: 1,
            candidate_families: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            hydrated_family_count: None,
            retrieval_stage_count: None,
            unknown_reason: None,
            active_generation: Some("gen-000002".to_string()),
            alignment_status: None,
        }
    }

    fn abstain_actual() -> EvalActual {
        EvalActual {
            outcome: EvalOutcome::Unknown,
            route: Some("discovery_unknown".to_string()),
            selected_family: None,
            candidate_family_count: 0,
            candidate_families: Vec::new(),
            hydrated_family_count: None,
            retrieval_stage_count: None,
            unknown_reason: Some("InsufficientSupport".to_string()),
            active_generation: Some("gen-000002".to_string()),
            alignment_status: None,
        }
    }

    #[test]
    fn product_eval_matcher_accepts_found_expectation() {
        let expected = EvalExpected {
            outcome: Some(EvalOutcome::Ok),
            family_prefix: Some("family:python:fastapi_route".to_string()),
            ..EvalExpected::default()
        };
        let (is_match, fields) = evaluate_match(&expected, &sample_found_actual());
        assert!(is_match);
        assert!(fields.is_empty());
        assert!(!is_false_family_selection(
            &expected,
            &sample_found_actual()
        ));
    }

    #[test]
    fn product_eval_matcher_flags_wrong_family_as_false_selection() {
        let expected = EvalExpected {
            outcome: Some(EvalOutcome::Ok),
            family_prefix: Some("family:python:fastapi_route".to_string()),
            ..EvalExpected::default()
        };
        let actual = EvalActual {
            selected_family: Some(
                "family:python:pydantic_model:framework_pydantic_model".to_string(),
            ),
            ..sample_found_actual()
        };
        let (is_match, fields) = evaluate_match(&expected, &actual);
        assert!(!is_match);
        assert_eq!(fields, vec!["family_prefix".to_string()]);
        assert!(is_false_family_selection(&expected, &actual));
    }

    #[test]
    fn product_eval_matcher_supports_any_of_and_unconstrained_fields() {
        let actual = EvalActual {
            selected_family: Some(
                "family:python:pydantic_model:framework_pydantic_model".to_string(),
            ),
            ..sample_found_actual()
        };
        let any_of = EvalExpected {
            family_any_of: Some(vec![
                "family:python:sqlalchemy_model".to_string(),
                "family:python:pydantic_model".to_string(),
            ]),
            ..EvalExpected::default()
        };
        let (is_match, _) = evaluate_match(&any_of, &actual);
        assert!(is_match);

        let unconstrained = EvalExpected::default();
        let (is_match, fields) = evaluate_match(&unconstrained, &actual);
        assert!(is_match);
        assert!(fields.is_empty());
    }

    #[test]
    fn product_eval_matcher_enforces_unknown_reason_constraint() {
        let expected = EvalExpected {
            outcome: Some(EvalOutcome::Unknown),
            unknown_reason: Some("StaleEvidence".to_string()),
            route: Some("exact_lookup_unknown".to_string()),
            ..EvalExpected::default()
        };
        let stale = EvalActual {
            outcome: EvalOutcome::Unknown,
            route: Some("exact_lookup_unknown".to_string()),
            selected_family: None,
            candidate_family_count: 0,
            candidate_families: Vec::new(),
            hydrated_family_count: None,
            retrieval_stage_count: None,
            unknown_reason: Some("StaleEvidence".to_string()),
            active_generation: Some("gen-000002".to_string()),
            alignment_status: None,
        };
        let (is_match, _) = evaluate_match(&expected, &stale);
        assert!(is_match);
        assert!(!is_false_family_selection(&expected, &stale));

        let wrong = EvalActual {
            unknown_reason: Some("InsufficientSupport".to_string()),
            ..stale
        };
        let (is_match, fields) = evaluate_match(&expected, &wrong);
        assert!(!is_match);
        assert!(fields.contains(&"unknown_reason".to_string()));
    }

    #[test]
    fn product_eval_fixture_hash_is_order_independent() {
        let first = TempRoot::new("eval-hash-a");
        write_file(first.path().join("sub/one.py"), b"alpha");
        write_file(first.path().join("two.py"), b"beta");

        let second = TempRoot::new("eval-hash-b");
        write_file(second.path().join("two.py"), b"beta");
        write_file(second.path().join("sub/one.py"), b"alpha");

        let first_hash = fixture_version_hash(first.path()).expect("hash first");
        let second_hash = fixture_version_hash(second.path()).expect("hash second");
        assert_eq!(first_hash, second_hash);

        write_file(second.path().join("two.py"), b"beta-changed");
        let changed_hash = fixture_version_hash(second.path()).expect("hash changed");
        assert_ne!(first_hash, changed_hash);
    }

    #[test]
    fn product_eval_time_and_percentile_helpers_are_deterministic() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_600_000_000), "2020-09-13T12:26:40Z");
        assert_eq!(percentile(&[], 50), 0);
        assert_eq!(percentile(&[5u128], 95), 5);
        assert_eq!(percentile(&[10u128, 20, 30], 50), 20);
        assert_eq!(percentile(&[10u128, 20, 30, 40], 95), 40);
    }

    #[test]
    fn product_eval_result_entry_round_trips_required_fields() {
        let expected = EvalExpected {
            outcome: Some(EvalOutcome::Ok),
            family_prefix: Some("family:python:fastapi_route".to_string()),
            ..EvalExpected::default()
        };
        let actual = EvalActual {
            outcome: EvalOutcome::Unknown,
            route: Some("discovery_unknown".to_string()),
            selected_family: None,
            candidate_family_count: 0,
            candidate_families: Vec::new(),
            hydrated_family_count: None,
            retrieval_stage_count: None,
            unknown_reason: Some("InsufficientSupport".to_string()),
            active_generation: Some("gen-000002".to_string()),
            alignment_status: None,
        };
        let (is_match, mismatch_fields) = evaluate_match(&expected, &actual);
        let entry = serde_json::json!({
            "query_id": "py-nl-fastapi-routes",
            "fixture_id": "python-v0_1",
            "kind": "nl_pattern_question",
            "operation": "find",
            "target": "How are FastAPI routes implemented?",
            "expected": expected.to_value(),
            "actual": actual.to_value(),
            "match": is_match,
            "mismatch_fields": mismatch_fields,
            "latency_ms_all_reps": [10u128, 12, 11],
            "latency_ms_p50": percentile(&[10u128, 12, 11], 50),
        });
        let serialized = serde_json::to_string(&entry).expect("serialize entry");
        let parsed: serde_json::Value = serde_json::from_str(&serialized).expect("parse entry");
        assert_eq!(parsed["match"], false);
        assert_eq!(parsed["actual"]["outcome"], "unknown");
        assert!(parsed["actual"]["selected_family"].is_null());
        assert_eq!(parsed["actual"]["candidate_family_count"], 0);
        assert_eq!(parsed["actual"]["unknown_reason"], "InsufficientSupport");
        assert_eq!(
            parsed["expected"]["family_prefix"],
            "family:python:fastapi_route"
        );
        assert_eq!(parsed["latency_ms_p50"], 11);
        assert!(parsed["mismatch_fields"]
            .as_array()
            .expect("mismatch fields array")
            .iter()
            .any(|field| field == "outcome"));
    }

    #[test]
    fn product_eval_corpus_parses_intent_and_candidates_include() {
        let value = serde_json::json!({
            "schema_version": "product-eval-corpus.v1",
            "fixtures": [{"fixture_id": "f1", "root": "src/fixtures/x", "description": "d"}],
            "queries": [{
                "query_id": "q1",
                "fixture_id": "f1",
                "kind": "ambiguous",
                "intent": "abstention",
                "operation": "find",
                "target": "app.py",
                "mode": "compact",
                "expected": {
                    "outcome": "unknown",
                    "candidates_include": [
                        "family:python:fastapi_route",
                        "family:python:pydantic_model"
                    ]
                }
            }]
        });
        let corpus = EvalCorpus::from_value(&value).expect("corpus parses");
        assert_eq!(corpus.queries[0].intent, Some(EvalIntent::Abstention));
        assert_eq!(
            corpus.queries[0].expected.candidates_include,
            Some(vec![
                "family:python:fastapi_route".to_string(),
                "family:python:pydantic_model".to_string(),
            ])
        );
    }

    #[test]
    fn product_eval_intent_is_optional_and_rejects_unknown_token() {
        // Absent intent stays None so a legacy v1 corpus keeps parsing.
        let legacy = serde_json::json!({
            "schema_version": "product-eval-corpus.v1",
            "fixtures": [{"fixture_id": "f1", "root": "r", "description": "d"}],
            "queries": [{
                "query_id": "q1",
                "fixture_id": "f1",
                "kind": "exact_path",
                "operation": "find",
                "target": "app.py",
                "mode": "compact",
                "expected": {"outcome": "ok"}
            }]
        });
        let corpus = EvalCorpus::from_value(&legacy).expect("legacy corpus parses");
        assert_eq!(corpus.queries[0].intent, None);

        let bad_intent = serde_json::json!({
            "schema_version": "product-eval-corpus.v1",
            "fixtures": [{"fixture_id": "f1", "root": "r", "description": "d"}],
            "queries": [{
                "query_id": "q1",
                "fixture_id": "f1",
                "kind": "exact_path",
                "intent": "recall",
                "operation": "find",
                "target": "app.py",
                "mode": "compact",
                "expected": {"outcome": "ok"}
            }]
        });
        assert!(EvalCorpus::from_value(&bad_intent).is_err());
    }

    #[test]
    fn product_eval_actual_reads_optional_count_fields() {
        let with_counts = serde_json::json!({
            "status": "ok",
            "active_generation": "gen-000009",
            "query_route": {
                "route": "discover_hydrate_compose",
                "selected_family_id": "family:python:fastapi_route:framework_fastapi_route",
                "candidate_family_ids": [
                    "family:python:fastapi_route:framework_fastapi_route"
                ],
                "hydrated_family_count": 3,
                "retrieval_stage_count": 4
            }
        });
        let actual = EvalActual::from_query_json(&with_counts).expect("parses");
        assert_eq!(actual.hydrated_family_count, Some(3));
        assert_eq!(actual.retrieval_stage_count, Some(4));

        // Absent count fields (current product) stay None and serialize as null.
        let without_counts = serde_json::json!({
            "status": "UNKNOWN",
            "query_route": {"route": "discovery_unknown", "candidate_family_ids": []}
        });
        let actual = EvalActual::from_query_json(&without_counts).expect("parses");
        assert_eq!(actual.hydrated_family_count, None);
        assert_eq!(actual.retrieval_stage_count, None);
        assert!(actual.to_value()["hydrated_family_count"].is_null());
        assert!(actual.to_value()["retrieval_stage_count"].is_null());
    }

    #[test]
    fn product_eval_reciprocal_rank_ranks_selected_then_candidates() {
        let expected = EvalExpected {
            family_prefix: Some("family:python:fastapi_route".to_string()),
            ..EvalExpected::default()
        };
        // Selected satisfies gold -> rank 1.
        assert_eq!(reciprocal_rank(&expected, &sample_found_actual()), 1.0);

        // Committed a wrong selection (outcome ok), gold appears second in the
        // candidate list -> 1/2. MRR credits the committed ranking.
        let committed_ranked = EvalActual {
            outcome: EvalOutcome::Ok,
            selected_family: Some(
                "family:python:pydantic_model:framework_pydantic_model".to_string(),
            ),
            candidate_families: vec![
                "family:python:pydantic_model:framework_pydantic_model".to_string(),
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ],
            ..sample_found_actual()
        };
        assert_eq!(reciprocal_rank(&expected, &committed_ranked), 0.5);

        // Abstention outcome scores 0 even when gold sits in the candidate list:
        // MRR measures the committed answer, not list construction.
        let abstained_with_gold = EvalActual {
            candidate_families: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            ..abstain_actual()
        };
        assert_eq!(reciprocal_rank(&expected, &abstained_with_gold), 0.0);

        // Gold only beyond the top-K committed candidates -> 0 (K = 5).
        let mut deep_candidates: Vec<String> = (0..5)
            .map(|index| format!("family:python:pydantic_model:framework_pydantic_model_{index}"))
            .collect();
        deep_candidates.push("family:python:fastapi_route:framework_fastapi_route".to_string());
        let committed_deep = EvalActual {
            outcome: EvalOutcome::Ok,
            selected_family: Some(
                "family:python:pydantic_model:framework_pydantic_model".to_string(),
            ),
            candidate_families: deep_candidates,
            ..sample_found_actual()
        };
        assert_eq!(reciprocal_rank(&expected, &committed_deep), 0.0);

        // Gold absent entirely -> 0.
        assert_eq!(reciprocal_rank(&expected, &abstain_actual()), 0.0);

        // No family constraint -> 0 regardless of selection.
        assert_eq!(
            reciprocal_rank(&EvalExpected::default(), &sample_found_actual()),
            0.0
        );
    }

    #[test]
    fn product_eval_candidate_recall_requires_every_prefix() {
        let includes = vec![
            "family:python:fastapi_route".to_string(),
            "family:python:pydantic_model".to_string(),
        ];
        let both = vec![
            "family:python:fastapi_route:framework_fastapi_route".to_string(),
            "family:python:pydantic_model:framework_pydantic_model:v9e3a0ddde854".to_string(),
        ];
        assert!(candidate_recall_satisfied(&includes, &both));
        let only_one = vec!["family:python:fastapi_route:framework_fastapi_route".to_string()];
        assert!(!candidate_recall_satisfied(&includes, &only_one));

        // A prefix matched only beyond the top-K candidates does not count (K = 5).
        let mut deep = vec!["family:python:fastapi_route:framework_fastapi_route".to_string()];
        for index in 0..5 {
            deep.push(format!("family:python:filler_{index}:framework_filler"));
        }
        deep.push("family:python:pydantic_model:framework_pydantic_model".to_string());
        assert!(!candidate_recall_satisfied(&includes, &deep));
    }

    #[test]
    fn product_eval_metrics_math_is_deterministic() {
        // Retrieval hit: selected family matches gold prefix.
        let retrieval_hit_expected = EvalExpected {
            outcome: Some(EvalOutcome::Ok),
            family_prefix: Some("family:python:fastapi_route".to_string()),
            ..EvalExpected::default()
        };
        let retrieval_hit_actual = sample_found_actual();

        // Retrieval gap: gold family exists but product abstains (the NL gap).
        let retrieval_gap_expected = EvalExpected {
            outcome: Some(EvalOutcome::Ok),
            family_prefix: Some("family:python:pytest_fixture".to_string()),
            ..EvalExpected::default()
        };
        let retrieval_gap_actual = abstain_actual();

        // Abstention with candidates surfaced: gold candidates_include is met.
        let abstain_expected = EvalExpected {
            outcome: Some(EvalOutcome::Unknown),
            candidates_include: Some(vec![
                "family:python:fastapi_route".to_string(),
                "family:python:pydantic_model".to_string(),
            ]),
            ..EvalExpected::default()
        };
        let abstain_actual_candidates = EvalActual {
            candidate_family_count: 2,
            candidate_families: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
                "family:python:pydantic_model:framework_pydantic_model:v9e3a0ddde854".to_string(),
            ],
            ..abstain_actual()
        };

        // Unsupported concept: correctly rejected as unknown.
        let unsupported_expected = EvalExpected {
            outcome: Some(EvalOutcome::Unknown),
            ..EvalExpected::default()
        };

        let expected_slots = [
            retrieval_hit_expected,
            retrieval_gap_expected,
            abstain_expected,
            unsupported_expected,
        ];
        let actual_slots = [
            retrieval_hit_actual,
            retrieval_gap_actual,
            abstain_actual_candidates,
            abstain_actual(),
        ];
        let records = vec![
            EvalMetricRecord {
                intent: Some(EvalIntent::Retrieval),
                kind: "nl_pattern_question",
                expected: &expected_slots[0],
                actual: &actual_slots[0],
                is_match: true,
            },
            EvalMetricRecord {
                intent: Some(EvalIntent::Retrieval),
                kind: "nl_pattern_question",
                expected: &expected_slots[1],
                actual: &actual_slots[1],
                is_match: false,
            },
            EvalMetricRecord {
                intent: Some(EvalIntent::Abstention),
                kind: "ambiguous",
                expected: &expected_slots[2],
                actual: &actual_slots[2],
                is_match: true,
            },
            EvalMetricRecord {
                intent: Some(EvalIntent::Abstention),
                kind: "unsupported_concept",
                expected: &expected_slots[3],
                actual: &actual_slots[3],
                is_match: true,
            },
        ];
        let metrics = compute_product_eval_metrics(&records);

        // Hit@1 over the two retrieval queries: one hit.
        assert_eq!((metrics.hit_at_1_num, metrics.hit_at_1_den), (1, 2));
        // MRR: 1.0 (hit) + 0.0 (gap) over 2 retrieval queries -> 0.5.
        assert_eq!(metrics.mrr_den, 2);
        assert_eq!(metrics.mrr_sum, 1.0);
        // Candidate recall: only the abstention query declares candidates_include.
        assert_eq!(
            (metrics.candidate_recall_num, metrics.candidate_recall_den),
            (1, 1)
        );
        // Correct abstention: both abstention queries returned unknown.
        assert_eq!(
            (
                metrics.correct_abstention_num,
                metrics.correct_abstention_den
            ),
            (2, 2)
        );
        // Only the retrieval-hit query carries a satisfied family constraint;
        // no false family selections anywhere.
        assert_eq!(metrics.false_family_selections, 0);
        assert_eq!(metrics.family_constrained_total, 2);
        // Every abstention-gold query here correctly abstains: no confident wrong
        // selections on abstention gold.
        assert_eq!(metrics.selected_on_abstention_gold, 0);
        // Unsupported rejection: the one unsupported_concept query is unknown.
        assert_eq!(
            (
                metrics.unsupported_rejection_num,
                metrics.unsupported_rejection_den
            ),
            (1, 1)
        );
        // Ambiguity precision: the ambiguous abstention query is unknown.
        assert_eq!(
            (
                metrics.ambiguity_precision_num,
                metrics.ambiguity_precision_den
            ),
            (1, 1)
        );
        assert_eq!(metrics.by_intent.get("retrieval"), Some(&(2, 1)));
        assert_eq!(metrics.by_intent.get("abstention"), Some(&(2, 2)));

        let value = metrics.to_value();
        assert_eq!(value["hit_at_1"], serde_json::json!(0.5));
        assert_eq!(value["mrr"], serde_json::json!(0.5));
        assert_eq!(value["candidate_recall"], serde_json::json!(1.0));
        assert_eq!(value["false_family_rate"], serde_json::json!(0.0));
    }

    #[test]
    fn product_eval_metrics_empty_denominators_serialize_null() {
        let metrics = compute_product_eval_metrics(&[]);
        let value = metrics.to_value();
        assert!(value["hit_at_1"].is_null());
        assert!(value["mrr"].is_null());
        assert!(value["candidate_recall"].is_null());
        assert!(value["correct_abstention_rate"].is_null());
        assert!(value["false_family_rate"].is_null());
        assert!(value["unsupported_rejection_rate"].is_null());
        assert!(value["ambiguity_precision"].is_null());
    }

    #[test]
    fn product_eval_condition_token_validation() {
        for valid in [
            "product",
            "baseline_token_overlap",
            "ablation-1",
            "abc123",
            &"a".repeat(40),
        ] {
            assert_eq!(
                validate_condition_token(valid).expect("token accepted"),
                valid
            );
        }
        for invalid in [
            "",
            "Product",
            "has space",
            "a.b",
            "trailing\n",
            "-leading",
            "--baseline",
            "-",
            &"a".repeat(41),
        ] {
            assert!(
                validate_condition_token(invalid).is_err(),
                "expected '{invalid}' to be rejected"
            );
        }
    }

    #[test]
    fn product_eval_baseline_token_maps_to_default_condition() {
        assert_eq!(
            EvalBaseline::from_token("token-overlap").expect("baseline token"),
            EvalBaseline::TokenOverlap
        );
        assert!(EvalBaseline::from_token("bm25").is_err());
        assert_eq!(
            EvalBaseline::TokenOverlap.default_condition(),
            "baseline_token_overlap"
        );
        assert_eq!(EvalBaseline::TokenOverlap.as_str(), "token-overlap");
    }

    #[test]
    fn product_eval_resolve_condition_couples_baseline_and_condition() {
        // Defaults: plain product, and a baseline's own default condition.
        assert_eq!(
            resolve_eval_condition(None, None).expect("product default"),
            "product"
        );
        assert_eq!(
            resolve_eval_condition(None, Some(EvalBaseline::TokenOverlap))
                .expect("baseline default"),
            "baseline_token_overlap"
        );
        // An explicit token wins verbatim, with or without a baseline.
        assert_eq!(
            resolve_eval_condition(Some("ablation-1".to_string()), None).expect("explicit"),
            "ablation-1"
        );
        assert_eq!(
            resolve_eval_condition(
                Some("ablation-1".to_string()),
                Some(EvalBaseline::TokenOverlap)
            )
            .expect("labeled baseline"),
            "ablation-1"
        );
        // Explicit `product` alone is fine; with a baseline it is rejected.
        assert_eq!(
            resolve_eval_condition(Some("product".to_string()), None).expect("explicit product"),
            "product"
        );
        assert!(resolve_eval_condition(
            Some("product".to_string()),
            Some(EvalBaseline::TokenOverlap)
        )
        .is_err());
    }

    #[test]
    fn product_eval_selected_on_abstention_gold_counts_confident_wrong_selections() {
        // Abstention gold where a family is nonetheless selected: counted, even
        // though the query declares no family constraint so false_family stays 0.
        let abstention_expected = EvalExpected {
            outcome: Some(EvalOutcome::Unknown),
            ..EvalExpected::default()
        };
        let selected_actual = EvalActual {
            outcome: EvalOutcome::Ok,
            selected_family: Some(
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ),
            ..sample_found_actual()
        };
        // A correctly abstaining run on the same gold is not counted.
        let abstained_actual = abstain_actual();
        let records = vec![
            EvalMetricRecord {
                intent: Some(EvalIntent::Abstention),
                kind: "typo_unsafe",
                expected: &abstention_expected,
                actual: &selected_actual,
                is_match: false,
            },
            EvalMetricRecord {
                intent: Some(EvalIntent::Abstention),
                kind: "ambiguous",
                expected: &abstention_expected,
                actual: &abstained_actual,
                is_match: true,
            },
        ];
        let metrics = compute_product_eval_metrics(&records);
        assert_eq!(metrics.selected_on_abstention_gold, 1);
        assert_eq!(metrics.false_family_selections, 0);
        assert_eq!(metrics.family_constrained_total, 0);
        assert_eq!(metrics.to_value()["selected_on_abstention_gold"], 1);
    }

    #[test]
    fn product_eval_baseline_tokenize_lowercases_splits_and_dedups() {
        assert_eq!(
            baseline_tokenize("How are FastAPI routes implemented?"),
            vec!["how", "are", "fastapi", "routes", "implemented"]
        );
        // `py` (2 chars) and `3` (1 char) fall below the minimum length.
        assert_eq!(baseline_tokenize("app.py:3"), vec!["app"]);
        // The repeated `fastapi`/`route` tokens are recorded once each.
        assert_eq!(
            baseline_tokenize("family:python:fastapi_route:framework_fastapi_route"),
            vec!["family", "python", "fastapi", "route", "framework"]
        );
    }

    #[test]
    fn product_eval_baseline_score_counts_distinct_substring_tokens() {
        let tokens = vec![
            "fastapi".to_string(),
            "route".to_string(),
            "xyz".to_string(),
        ];
        assert_eq!(
            baseline_family_score(
                &tokens,
                "family:python:fastapi_route:framework_fastapi_route"
            ),
            2
        );
        assert_eq!(
            baseline_family_score(
                &tokens,
                "family:python:pydantic_model:framework_pydantic_model"
            ),
            0
        );
    }

    #[test]
    fn product_eval_baseline_selects_unique_argmax_above_threshold() {
        let tokens = baseline_tokenize("family:python:fastapi_route:framework_fastapi_route");
        let families = vec![
            "family:python:fastapi_route:framework_fastapi_route".to_string(),
            "family:python:pydantic_model:framework_pydantic_model".to_string(),
        ];
        let selection = baseline_select_family(&tokens, &families);
        assert_eq!(
            selection.selected_family.as_deref(),
            Some("family:python:fastapi_route:framework_fastapi_route")
        );
        // Both families share the structural `family`/`python` tokens, so both are
        // ranked candidates; the fastapi family sorts first on its higher score.
        assert_eq!(
            selection.candidate_families,
            vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
                "family:python:pydantic_model:framework_pydantic_model".to_string(),
            ]
        );
    }

    #[test]
    fn product_eval_baseline_abstains_on_strict_tie() {
        // Only the structural prefix tokens match, and they match both families
        // equally, so the strict-tie rule abstains while still listing candidates.
        let tokens = vec!["family".to_string(), "python".to_string()];
        let families = vec![
            "family:python:fastapi_route:framework_fastapi_route".to_string(),
            "family:python:pydantic_model:framework_pydantic_model".to_string(),
        ];
        let selection = baseline_select_family(&tokens, &families);
        assert!(selection.selected_family.is_none());
        assert_eq!(selection.candidate_families.len(), 2);
    }

    #[test]
    fn product_eval_baseline_abstains_below_threshold() {
        // A unique max of 1 is below the score-of-2 selection threshold.
        let tokens = vec!["fastapi".to_string()];
        let families = vec![
            "family:python:fastapi_route:framework_fastapi_route".to_string(),
            "family:python:pydantic_model:framework_pydantic_model".to_string(),
        ];
        let selection = baseline_select_family(&tokens, &families);
        assert!(selection.selected_family.is_none());
        assert_eq!(
            selection.candidate_families,
            vec!["family:python:fastapi_route:framework_fastapi_route".to_string()]
        );
    }

    #[test]
    fn product_eval_baseline_ranking_is_deterministic_and_capped() {
        let tokens = vec!["alpha".to_string(), "beta".to_string()];
        let families: Vec<String> = (0..7).map(|index| format!("alpha_beta_{index}")).collect();
        let mut shuffled = families.clone();
        shuffled.reverse();
        let forward = baseline_select_family(&tokens, &families);
        let reversed = baseline_select_family(&tokens, &shuffled);
        // Seven families tie at score 2, so selection abstains; the candidate list
        // is capped at five and ordered by id regardless of input order.
        assert!(forward.selected_family.is_none());
        assert_eq!(forward.candidate_families, reversed.candidate_families);
        assert_eq!(
            forward.candidate_families,
            vec![
                "alpha_beta_0".to_string(),
                "alpha_beta_1".to_string(),
                "alpha_beta_2".to_string(),
                "alpha_beta_3".to_string(),
                "alpha_beta_4".to_string(),
            ]
        );
    }

    #[test]
    fn product_eval_baseline_actual_projects_outcome() {
        let selected = baseline_actual(
            BaselineSelection {
                selected_family: Some("family:python:fastapi_route:x".to_string()),
                candidate_families: vec!["family:python:fastapi_route:x".to_string()],
            },
            Some("gen-000002".to_string()),
        );
        assert_eq!(selected.outcome, EvalOutcome::Ok);
        assert_eq!(selected.candidate_family_count, 1);
        assert!(selected.route.is_none());
        assert!(selected.unknown_reason.is_none());
        assert_eq!(selected.active_generation.as_deref(), Some("gen-000002"));

        let abstained = baseline_actual(
            BaselineSelection {
                selected_family: None,
                candidate_families: Vec::new(),
            },
            None,
        );
        assert_eq!(abstained.outcome, EvalOutcome::Unknown);
        assert!(abstained.selected_family.is_none());
    }

    #[test]
    fn product_eval_results_serialize_condition_and_baseline_fields() {
        // Build a results document with the same top-level and summary shape
        // `run_product_eval` emits, so the new provenance/safety fields are
        // asserted end-to-end through real serialization (no product binary).
        let abstention_expected = EvalExpected {
            outcome: Some(EvalOutcome::Unknown),
            ..EvalExpected::default()
        };
        let selected_actual = EvalActual {
            outcome: EvalOutcome::Ok,
            selected_family: Some(
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ),
            ..sample_found_actual()
        };
        let records = vec![EvalMetricRecord {
            intent: Some(EvalIntent::Abstention),
            kind: "typo_unsafe",
            expected: &abstention_expected,
            actual: &selected_actual,
            is_match: false,
        }];
        let metrics = compute_product_eval_metrics(&records);

        let build = |baseline: Option<EvalBaseline>,
                     explicit: Option<String>|
         -> serde_json::Value {
            let condition = resolve_eval_condition(explicit, baseline).expect("condition resolves");
            serde_json::json!({
                "schema_version": PRODUCT_EVAL_RESULTS_SCHEMA,
                "condition": condition,
                "baseline": baseline.map(EvalBaseline::as_str),
                "repetitions": 1,
                "summary": {
                    "total": 1,
                    "false_family_selections": metrics.false_family_selections,
                    "selected_on_abstention_gold": metrics.selected_on_abstention_gold,
                    "metrics": metrics.to_value(),
                },
            })
        };

        // Baseline run: condition defaults, baseline field names the control, and
        // the safety counter is surfaced both at summary top level and in metrics.
        let baseline_doc = build(Some(EvalBaseline::TokenOverlap), None);
        let parsed: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&baseline_doc).expect("serialize"))
                .expect("parse");
        assert_eq!(parsed["schema_version"], PRODUCT_EVAL_RESULTS_SCHEMA);
        assert_eq!(parsed["condition"], "baseline_token_overlap");
        assert_eq!(parsed["baseline"], "token-overlap");
        assert_eq!(parsed["summary"]["selected_on_abstention_gold"], 1);
        assert_eq!(
            parsed["summary"]["metrics"]["selected_on_abstention_gold"],
            1
        );
        assert_eq!(parsed["summary"]["metrics"]["false_family_selections"], 0);

        // Product run: baseline field is explicit null.
        let product_doc = build(None, None);
        let parsed_product: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&product_doc).expect("serialize"))
                .expect("parse");
        assert_eq!(parsed_product["condition"], "product");
        assert!(parsed_product["baseline"].is_null());
    }

    #[test]
    fn canonical_json_string_is_stable_and_discriminating() {
        let left: serde_json::Value =
            serde_json::from_str(r#"{"b":1,"a":[3,{"y":2,"x":1}]}"#).expect("parse left");
        let right: serde_json::Value =
            serde_json::from_str(r#"{"a":[3,{"x":1,"y":2}],"b":1}"#).expect("parse right");
        assert_eq!(canonical_json_string(&left), canonical_json_string(&right));
        let changed: serde_json::Value =
            serde_json::from_str(r#"{"a":[3,{"x":9,"y":2}],"b":1}"#).expect("parse changed");
        assert_ne!(
            canonical_json_string(&left),
            canonical_json_string(&changed)
        );
    }

    #[test]
    fn sync_equivalence_multiset_diff_has_teeth() {
        let incremental = vec!["a".to_string(), "b".to_string(), "b".to_string()];
        let clean = vec!["a".to_string(), "b".to_string()];
        let (b_only, c_only) = sync_equivalence_multiset_diff(&incremental, &clean);
        assert_eq!(b_only, vec!["b".to_string()]);
        assert!(c_only.is_empty());

        let (same_b, same_c) = sync_equivalence_multiset_diff(&incremental, &incremental);
        assert!(same_b.is_empty() && same_c.is_empty());

        assert!(!sync_equivalence_surface("code_units", &incremental, &clean).equal);
        assert!(sync_equivalence_surface("code_units", &incremental, &incremental).equal);
    }

    #[test]
    fn sync_equivalence_provider_origin_classification() {
        // Only the external TypeScript worker is exempted from equality; the
        // in-binary Rust provider (`cargo_metadata`) is reproducible in the clean
        // rebuild and must be compared like any local engine.
        assert!(sync_equivalence_is_worker_provider_origin("typescript"));
        assert!(!sync_equivalence_is_worker_provider_origin(
            "cargo_metadata"
        ));
        assert!(!sync_equivalence_is_worker_provider_origin("python"));
        assert!(!sync_equivalence_is_worker_provider_origin(
            "repogrammar-java-syntax"
        ));
        assert!(!sync_equivalence_is_worker_provider_origin(
            "repogrammar-tsjs-syntax"
        ));
    }

    #[test]
    fn sync_equivalence_scenario_pass_gate_is_not_trivially_satisfiable() {
        // Expected EQUAL, incremental path, no reparsed-count expectation: passes.
        assert!(sync_equivalence_scenario_passes(
            "EQUAL", None, None, "EQUAL", None, None
        ));
        // Expected FELL_BACK with a reason: passes on the exact reason.
        assert!(sync_equivalence_scenario_passes(
            "FELL_BACK",
            Some("project_context_changed"),
            None,
            "FELL_BACK",
            Some("project_context_changed"),
            None,
        ));
        // Expected EQUAL with a reparsed-count expectation that matches: passes.
        assert!(sync_equivalence_scenario_passes(
            "EQUAL",
            None,
            Some(1),
            "EQUAL",
            None,
            Some(1),
        ));
        // Gate regression: an expected fallback silently ran incrementally and
        // was (vacuously) EQUAL — must FAIL.
        assert!(!sync_equivalence_scenario_passes(
            "EQUAL",
            None,
            None,
            "FELL_BACK",
            Some("project_context_changed"),
            None,
        ));
        // Preflight misfire: an expected-incremental scenario fell back — fail.
        assert!(!sync_equivalence_scenario_passes(
            "FELL_BACK",
            Some("engine_version_changed"),
            None,
            "EQUAL",
            None,
            None,
        ));
        // Right outcome, wrong reason — fail.
        assert!(!sync_equivalence_scenario_passes(
            "FELL_BACK",
            Some("engine_version_changed"),
            None,
            "FELL_BACK",
            Some("project_context_changed"),
            None,
        ));
        // Right outcome, but reparsed more files than the file-local path should —
        // fail (this is what catches a fast path that silently reparsed too much).
        assert!(!sync_equivalence_scenario_passes(
            "EQUAL",
            None,
            Some(3),
            "EQUAL",
            None,
            Some(1),
        ));
        // Any inequality — fail regardless of expectation.
        assert!(!sync_equivalence_scenario_passes(
            "INEQUAL", None, None, "EQUAL", None, None
        ));
    }

    #[test]
    fn sync_equivalence_fact_tuple_distinguishes_hash_target_and_assumptions() {
        use repogrammar::core::model::ContentHash;
        use repogrammar::ports::index_store::IndexedSemanticFactRecord;
        let base = || IndexedSemanticFactRecord {
            fact_id: "semantic-fact:1".to_string(),
            kind: "FRAMEWORK_ROLE".to_string(),
            subject: "s".to_string(),
            target: None,
            certainty: "FRAMEWORK_HEURISTIC".to_string(),
            origin_engine: "python".to_string(),
            origin_engine_version: "1".to_string(),
            origin_method: "m".to_string(),
            assumptions: vec!["a".to_string(), "b".to_string()],
            evidence_id: "semantic-evidence:1".to_string(),
            code_unit_id: "unit:x".to_string(),
            path: "x.py".to_string(),
            content_hash: ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
            start_byte: 0,
            end_byte: 10,
            note: "n".to_string(),
        };
        // Sequence ids are excluded, so tuples ignore fact_id/evidence_id.
        let mut only_ids = base();
        only_ids.fact_id = "semantic-fact:99".to_string();
        only_ids.evidence_id = "semantic-evidence:99".to_string();
        assert_eq!(
            sync_equivalence_fact_tuple(&base()),
            sync_equivalence_fact_tuple(&only_ids)
        );
        // content_hash is part of the tuple (stale-fact detection).
        let mut other_hash = base();
        other_hash.content_hash =
            ContentHash::new(format!("sha256:{}", "b".repeat(64))).expect("hash");
        assert_ne!(
            sync_equivalence_fact_tuple(&base()),
            sync_equivalence_fact_tuple(&other_hash)
        );
        // target=None and target=Some("") do not collapse.
        let mut empty_target = base();
        empty_target.target = Some(String::new());
        assert_ne!(
            sync_equivalence_fact_tuple(&base()),
            sync_equivalence_fact_tuple(&empty_target)
        );
        // ["a","b"] and ["a,b"] do not canonicalize alike (unit-separator join).
        let mut joined_assumptions = base();
        joined_assumptions.assumptions = vec!["a,b".to_string()];
        assert_ne!(
            sync_equivalence_fact_tuple(&base()),
            sync_equivalence_fact_tuple(&joined_assumptions)
        );
    }

    #[test]
    fn resolve_sync_equivalence_fixture_requires_a_directory() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let error = resolve_sync_equivalence_fixture(manifest, "Cargo.toml")
            .expect_err("a regular file is not a fixture root");
        assert!(error.contains("directory"));
        resolve_sync_equivalence_fixture(manifest, "src/fixtures/incremental_equivalence/v1")
            .expect("committed oracle fixture resolves");
    }

    #[test]
    fn sync_equivalence_scenario_patches_apply_to_committed_fixture() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/incremental_equivalence/v1");
        for scenario in SYNC_EQUIVALENCE_SCENARIOS {
            let scratch = unique_smoke_root().with_file_name(format!(
                "repogrammar-sync-equivalence-patch-{}-{}",
                std::process::id(),
                scenario.id
            ));
            let project = scratch.join("project");
            fs::create_dir_all(&project).expect("create scratch project");
            copy_dir_sorted(&source, &project).expect("copy committed fixture");
            (scenario.apply)(&project).unwrap_or_else(|error| panic!("{}: {error}", scenario.id));
            let read = |relative: &str| {
                fs::read_to_string(project.join(relative)).expect("read patched file")
            };
            match scenario.id {
                "java_edit" => {
                    assert!(read("service/java/OrderServiceTest.java").contains("assert 1 == 1;"))
                }
                "csharp_edit" => {
                    assert!(read("service/csharp/CatalogTests.cs").contains("Assert.True(1 == 1);"))
                }
                "docs_noop" => {
                    assert!(read("docs/NOTES.md").contains("docs-only no-op scenario"))
                }
                "rs_content_edit" => {
                    assert!(
                        read("service/rust/order_service.rs").contains("place_order(\"renamed\")")
                    )
                }
                "tsjs_content_edit" => assert!(read("web/users.test.ts").contains("return;")),
                "tsjs_add" => assert!(project.join("web/payments.test.ts").is_file()),
                "rs_add" => assert!(project.join("service/rust/extra_service.rs").is_file()),
                "mocharc_remove" => assert!(!project.join(".mocharc.json").exists()),
                "python_body_edit" => {
                    assert!(read("analytics/app.py").contains("return \"primary\""))
                }
                "python_interface_edit" => {
                    assert!(read("analytics/app.py").contains("def new_public_helper()"))
                }
                "python_conftest_edit" => {
                    assert!(read("analytics/conftest.py").contains("return \"primary\""))
                }
                "java_add" => {
                    assert!(project.join("service/java/ExtraServiceTest.java").is_file())
                }
                "java_delete" => {
                    assert!(!project.join("service/java/OrderServiceTest.java").exists())
                }
                other => panic!("unhandled scenario '{other}'"),
            }
            let _ = fs::remove_dir_all(&scratch);
        }
    }

    #[test]
    fn attribute_field_bytes_sorts_fields_and_measures_compact_bytes() {
        let value = serde_json::json!({
            "z_last": [1, 2, 3],
            "a_first": "value",
            "middle": {"k": 1},
        });
        let attribution = attribute_field_bytes(&value);
        // Sorted key order is stable regardless of insertion order.
        let keys: Vec<&String> = attribution.keys().collect();
        assert_eq!(keys, vec!["a_first", "middle", "z_last"]);
        // Compact serialized byte lengths, matching the audit's `vbytes`.
        assert_eq!(attribution["a_first"], "\"value\"".len() as u64);
        assert_eq!(attribution["z_last"], "[1,2,3]".len() as u64);
        assert_eq!(attribution["middle"], "{\"k\":1}".len() as u64);
        // A non-object payload attributes nothing.
        assert!(attribute_field_bytes(&serde_json::json!("scalar")).is_empty());
    }

    #[test]
    fn payload_measure_row_extracts_status_route_and_field_bytes() {
        let value = serde_json::json!({
            "status": "ok",
            "query_route": {"route": "discover_hydrate_compose"},
            "family": {"family_id": "family:python:fastapi_route:framework_fastapi_route"},
        });
        let (row, field_bytes) = payload_measure_row(
            "find",
            "found_big_family_path",
            "deep",
            "minimal",
            "on",
            "cli_json",
            &value,
            7617,
        );
        assert_eq!(row["operation"], "find");
        assert_eq!(row["category"], "found_big_family_path");
        assert_eq!(row["mode"], "deep");
        assert_eq!(row["verbosity"], "minimal");
        assert_eq!(row["source_spans"], "on");
        assert_eq!(row["surface"], "cli_json");
        assert_eq!(row["status"], "ok");
        assert_eq!(row["route"], "discover_hydrate_compose");
        assert_eq!(row["total_bytes"], 7617);
        assert!(row["field_bytes"].is_object());
        // Aggregation payload carries every top-level field.
        let fields: std::collections::BTreeSet<String> =
            field_bytes.into_iter().map(|(field, _)| field).collect();
        assert!(fields.contains("family"));
        assert!(fields.contains("query_route"));
        // An abstention payload without a route yields a null route.
        let abstention = serde_json::json!({"status": "UNKNOWN"});
        let (abstention_row, _) = payload_measure_row(
            "find",
            "abstain_unknown_nl",
            "compact",
            "minimal",
            "off",
            "cli_json",
            &abstention,
            1411,
        );
        assert!(abstention_row["route"].is_null());
        assert_eq!(abstention_row["status"], "UNKNOWN");
        assert_eq!(abstention_row["source_spans"], "off");
    }

    #[test]
    fn payload_measure_cases_cover_the_required_report_variants() {
        let categories: std::collections::BTreeSet<&str> = PAYLOAD_MEASURE_CASES
            .iter()
            .map(|case| case.category)
            .collect();
        for required in [
            "found_big_family_path",
            "found_small_family_path",
            "found_big_family_nl",
            "found_typescript_family_path",
            "abstain_unknown_nl",
            "partial_context_path",
            "check_big_family_path",
            "family_big",
            "family_small",
        ] {
            assert!(categories.contains(required), "missing category {required}");
        }
        // Every case names a supported query verb and a non-empty target.
        for case in PAYLOAD_MEASURE_CASES {
            assert!(
                matches!(case.operation, "find" | "family" | "check"),
                "unexpected operation {}",
                case.operation
            );
            assert!(!case.target.is_empty());
        }
        assert_eq!(PAYLOAD_MEASURE_MODES, ["compact", "deep"]);
        assert_eq!(PAYLOAD_MEASURE_VERBOSITIES, ["minimal", "standard", "full"]);
        // The source-spans variant (S6 read_plan<->spans dedup target) is
        // measured on the big Found family and on conformance.
        let spans_cases: std::collections::BTreeSet<&str> = PAYLOAD_MEASURE_CASES
            .iter()
            .filter(|case| case.measure_source_spans)
            .map(|case| case.category)
            .collect();
        assert!(spans_cases.contains("found_big_family_path"));
        assert!(spans_cases.contains("check_big_family_path"));
        assert_eq!(payload_measure_source_span_rows(), spans_cases.len() * 3);
    }

    /// Locates the product `repogrammar` binary that `cargo test --workspace`
    /// builds alongside the test harness by walking up from the test executable
    /// (`target/<profile>/deps/`) to the profile directory. Returns `None` only
    /// when the binary is absent; the smoke test then fails loudly rather than
    /// passing silently, so a green CI never hides an unmeasured harness.
    fn locate_built_product_binary() -> Option<PathBuf> {
        let file_name = if cfg!(windows) {
            "repogrammar.exe"
        } else {
            "repogrammar"
        };
        let executable = std::env::current_exe().ok()?;
        let mut directory = executable.parent();
        for _ in 0..6 {
            let current = directory?;
            let candidate = current.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
            directory = current.parent();
        }
        None
    }

    #[test]
    fn payload_measure_is_deterministic_and_schema_stable_end_to_end() {
        let binary = locate_built_product_binary().expect(
            "product `repogrammar` binary not found next to the test harness; \
build it first with `cargo build --bin repogrammar` or run the mandated \
`cargo test --workspace --all-features` (which builds every bin)",
        );
        let binary = binary.to_str().expect("product binary path is UTF-8");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out_first = unique_product_eval_root();
        let out_second = unique_product_eval_root();

        run_payload_measure(root, out_first.to_str().unwrap(), Some(binary), None)
            .expect("first payload-measure run");
        run_payload_measure(root, out_second.to_str().unwrap(), Some(binary), None)
            .expect("second payload-measure run");

        let first = fs::read(out_first.join("payload-bytes.summary.json"))
            .expect("first summary is readable");
        let second = fs::read(out_second.join("payload-bytes.summary.json"))
            .expect("second summary is readable");
        assert_eq!(
            first, second,
            "payload-bytes.summary.json must be byte-identical across two runs of the same fixture"
        );

        let summary: serde_json::Value =
            serde_json::from_slice(&first).expect("summary parses as JSON");
        assert_eq!(summary["schema_version"], PAYLOAD_MEASURE_SCHEMA);
        assert_eq!(summary["fixture_id"], PAYLOAD_MEASURE_FIXTURE_ID);
        assert_eq!(
            summary["product_schema_version"],
            repogrammar::application::query::PRODUCT_SCHEMA_VERSION
        );

        let rows = summary["rows"].as_array().expect("rows is an array");
        // query cases x 2 modes x 3 verbosities, plus source-spans variants,
        // plus 1 readiness row.
        let expected_rows =
            PAYLOAD_MEASURE_CASES.len() * 6 + payload_measure_source_span_rows() + 1;
        assert_eq!(rows.len(), expected_rows);
        assert_eq!(
            summary["totals"]["row_count"].as_u64().unwrap(),
            rows.len() as u64
        );

        let categories: std::collections::BTreeSet<&str> = rows
            .iter()
            .filter_map(|row| row["category"].as_str())
            .collect();
        for required in [
            "found_big_family_path",
            "found_small_family_path",
            "found_big_family_nl",
            "abstain_unknown_nl",
            "partial_context_path",
            "check_big_family_path",
            "readiness",
        ] {
            assert!(
                categories.contains(required),
                "missing measured category {required}"
            );
        }

        for row in rows {
            assert!(
                row["total_bytes"].as_u64().unwrap() > 0,
                "every measured payload has positive bytes"
            );
            assert!(row["field_bytes"].is_object());
        }

        // The big family really has 31 members with the cap rendering 20; a
        // fixture drift that changed either would fail here.
        let shape = &summary["fixture_shape"];
        assert_eq!(shape["category"], "found_big_family_path");
        assert_eq!(
            shape["member_count"].as_u64(),
            Some(31),
            "big family must report 31 members"
        );
        assert_eq!(
            shape["members_rendered"].as_u64(),
            Some(20),
            "member cap must render exactly 20 members"
        );
        assert_eq!(shape["members_truncated"].as_bool(), Some(true));
        // The source_spans field carries the full membership under the read plan;
        // cross-check the found_big_family row's `members` byte attribution stays
        // present at both source_spans states.
        let spans_on_rows: Vec<&serde_json::Value> = rows
            .iter()
            .filter(|row| row["source_spans"] == "on")
            .collect();
        assert_eq!(
            spans_on_rows.len(),
            payload_measure_source_span_rows(),
            "one source-spans row per flagged case per verbosity"
        );
        for row in &spans_on_rows {
            assert_eq!(row["mode"], "deep", "source spans only render in deep mode");
            assert!(
                row["field_bytes"].get("source_spans").is_some(),
                "a spans-on row must carry a non-empty source_spans field group"
            );
        }

        // Readiness is measured through the lean MCP surface, not CLI `status`.
        let readiness = rows
            .iter()
            .find(|row| row["category"] == "readiness")
            .expect("readiness row present");
        assert_eq!(readiness["surface"], "mcp_tool");
        assert_eq!(readiness["operation"], "inspect_readiness");
        // Rows are sorted, so the artifact ordering is itself deterministic.
        let ordered: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    "{}|{}|{}|{}|{}",
                    row["operation"].as_str().unwrap_or(""),
                    row["category"].as_str().unwrap_or(""),
                    row["mode"].as_str().unwrap_or(""),
                    row["verbosity"].as_str().unwrap_or(""),
                    row["source_spans"].as_str().unwrap_or("")
                )
            })
            .collect();
        let mut sorted = ordered.clone();
        sorted.sort();
        assert_eq!(ordered, sorted, "rows must be emitted in sorted order");

        let _ = fs::remove_dir_all(&out_first);
        let _ = fs::remove_dir_all(&out_second);
    }
}
