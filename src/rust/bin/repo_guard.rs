use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
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
    "docs/roadmap.md",
    ".agents/memories/README.md",
    ".agents/memories/project-state.md",
    ".agents/memories/known-constraints.md",
    ".agents/memories/open-questions.md",
    ".github/workflows/ci.yml",
    "docs/specifications/semantic-workers.md",
    "src/rust/bin/repo_guard.rs",
];
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "c", "cc", "cpp", "h", "hpp", "go", "py", "js", "jsx", "ts", "tsx", "java", "kt", "kts",
    "sh", "bash", "zsh", "ps1", "sql",
];
const IGNORED_DIRS: &[&str] = &[".git", "target", ".codegraph", ".repogrammar"];
const IGNORED_DIR_PREFIXES: &[&str] = &[".repogrammar-"];

fn main() {
    let root = match env::current_dir() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            std::process::exit(1);
        }
    };
    let result = run(env::args().skip(1), &root);
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    std::process::exit(result.status);
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
        [] => CommandResult::err(format!("{}\n", usage())),
        _ => CommandResult::err(format!("unknown or invalid arguments\n{}\n", usage())),
    }
}

fn usage() -> &'static str {
    "Usage: repo-guard check | sync-agent-guides --from <AGENTS.md|CLAUDE.md> | check-diff --base <rev> --head <rev>"
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
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            IGNORED_DIRS.contains(&name)
                || IGNORED_DIR_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
        })
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
