//! CLI argument boundary for the `repogrammar` binary.

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
        [command] if command == "init" || command == "index" || command == "serve" => {
            CliOutput::failure(
                2,
                format!(
                    "repogrammar {command} is not implemented yet; repository bootstrap only defines the command boundary\n"
                ),
            )
        }
        [unknown, ..] => CliOutput::failure(2, format!("unknown command or option: {unknown}\n")),
    }
}

fn usage() -> String {
    "Usage: repogrammar [--version] <init|index|serve>\n".to_string()
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
    fn product_commands_fail_stably_until_implemented() {
        for command in ["init", "index", "serve"] {
            let output = run([command]);

            assert_eq!(output.status, 2);
            assert!(output.stderr.contains("not implemented yet"));
            assert!(output.stdout.is_empty());
        }
    }
}
