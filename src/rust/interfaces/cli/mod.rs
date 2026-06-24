//! CLI argument boundary for the `repogrammar` binary.

use crate::application::install::{plan_install, AgentTarget, InstallRequest, InstallScope};
use crate::application::repository::RepositoryStatus;

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
            handle_project_lifecycle(command, rest)
        }
        [command, rest @ ..] if is_query_command(command) => handle_query(command, rest),
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

fn handle_project_lifecycle(command: &str, rest: &[String]) -> CliOutput {
    if command == "logs" {
        if let Err(error) = parse_log_options(rest) {
            return CliOutput::failure(2, format!("{error}\n"));
        }
        return CliOutput::failure(
            2,
            "repogrammar logs is not implemented yet; repo-local logs must be redacted and rotated before exposure\n",
        );
    }

    if let Err(error) = parse_long_running_options(rest) {
        return CliOutput::failure(2, format!("{error}\n"));
    }

    match command {
        "status" => CliOutput::success(format!(
            "{}\n",
            RepositoryStatus::NotInitialized.as_human_message()
        )),
        "doctor" => CliOutput::success(
            "doctor: CLI command surface is available; repository index is not initialized\n",
        ),
        "init" | "index" | "sync" => CliOutput::failure(
            2,
            format!(
                "repogrammar {command} is not implemented yet; v0.1 requires typed progress events and atomic index generation activation before this command can write state\n"
            ),
        ),
        "uninit" | "unlock" => CliOutput::failure(
            2,
            format!(
                "repogrammar {command} is not implemented yet; repository state mutation requires receipt or lock ownership validation\n"
            ),
        ),
        _ => CliOutput::failure(2, format!("unknown project lifecycle command: {command}\n")),
    }
}

fn handle_query(command: &str, rest: &[String]) -> CliOutput {
    if let Err(error) = parse_query_options(rest) {
        return CliOutput::failure(2, format!("{error}\n"));
    }

    CliOutput::failure(
        2,
        format!(
            "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init\ncommand: repogrammar {command} is not implemented yet; query execution requires a validated pattern-family index\n"
        ),
    )
}

fn handle_installer(command: &str, rest: &[String]) -> CliOutput {
    if command == "serve" {
        if let Err(error) = parse_long_running_options(rest) {
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

fn parse_long_running_options(rest: &[String]) -> Result<(), String> {
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--progress" => {
                let Some(value) = rest.get(index + 1) else {
                    return Err("--progress requires auto, always, or never".to_string());
                };
                if !matches!(value.as_str(), "auto" | "always" | "never") {
                    return Err("--progress requires auto, always, or never".to_string());
                }
                index += 2;
            }
            "--json" | "--quiet" | "--verbose" | "--write-gitignore" | "--force" => index += 1,
            value if !value.starts_with('-') => index += 1,
            other => return Err(format!("unknown long-running option: {other}")),
        }
    }
    Ok(())
}

fn parse_log_options(rest: &[String]) -> Result<(), String> {
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--component" => {
                let Some(value) = rest.get(index + 1) else {
                    return Err("--component requires index, daemon, mcp, or telemetry".to_string());
                };
                if !matches!(value.as_str(), "index" | "daemon" | "mcp" | "telemetry") {
                    return Err("--component requires index, daemon, mcp, or telemetry".to_string());
                }
                index += 2;
            }
            "--since" => {
                if rest.get(index + 1).is_none() {
                    return Err("--since requires a duration".to_string());
                }
                index += 2;
            }
            "--tail" | "--redact" | "--json" | "--quiet" | "--verbose" => index += 1,
            other => return Err(format!("unknown logs option: {other}")),
        }
    }
    Ok(())
}

fn parse_query_options(rest: &[String]) -> Result<(), String> {
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--token-budget" => {
                if rest.get(index + 1).is_none() {
                    return Err(format!("{} requires a value", rest[index]));
                }
                index += 2;
            }
            "--json" | "--include-variations" | "--include-exceptions" => index += 1,
            value if !value.starts_with('-') => index += 1,
            other => return Err(format!("unknown query option: {other}")),
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_succeeds() {
        let output = run(["--version"]);

        assert_eq!(output.status, 0);
        assert!(output.stdout.starts_with("repogrammar "));
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn pattern_family_command_surface_is_recognized() {
        for command in [
            "find", "families", "family", "member", "explain", "check", "files", "units",
        ] {
            let output = run([command]);

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
        let output = run([
            "find",
            "--project",
            ".",
            "--token-budget",
            "8000",
            "--json",
            "--include-variations",
            "--include-exceptions",
            "src/user.ts",
        ]);

        assert_eq!(output.status, 2);
        assert!(output.stderr.starts_with(
            "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init\n"
        ));
        assert!(output
            .stderr
            .contains("query execution requires a validated pattern-family index"));
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
    fn long_running_options_are_accepted() {
        let output = run([
            "init",
            ".",
            "--progress",
            "always",
            "--json",
            "--verbose",
            "--write-gitignore",
        ]);

        assert_eq!(output.status, 2);
        assert!(output.stderr.contains("typed progress events"));
    }

    #[test]
    fn logs_options_are_accepted() {
        let output = run([
            "logs",
            "--tail",
            "--since",
            "1h",
            "--component",
            "index",
            "--redact",
        ]);

        assert_eq!(output.status, 2);
        assert!(output.stderr.contains("repo-local logs"));
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
}
