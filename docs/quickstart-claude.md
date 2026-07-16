# Claude Code Quickstart

This source-checkout flow installs RepoGrammar and configures the global Claude
Code MCP integration. Use it whenever the exact-version availability gate in
`quickstart.md` does not pass or contributor dogfood is desired.

## Install The Command

```text
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

The current Python analysis path requires Python 3.10 or newer as `python3`.

## Review And Apply Claude Code Wiring

Dry-run first:

```text
repogrammar install --target claude-code --scope global --dry-run --no-telemetry
```

Apply the global Claude Code integration only after reviewing the plan:

```text
repogrammar install --target claude-code --scope global --yes --no-telemetry
```

`claude` is accepted as an alias by the CLI, but public docs should prefer the
canonical target id `claude-code`.

## Initialize A Project

Run this inside each repository where you want Claude Code to use RepoGrammar:

```text
repogrammar init --yes --autosync
repogrammar status
```

`repogrammar init --yes` is the agent-safe noninteractive bootstrap and builds
the first active index by default. Add `--autosync` when Claude Code should keep
agent-written files available to later RepoGrammar queries.

After initialization, Claude Code should use the `repogrammar_context` MCP tool
for implementation-pattern analogues, family detail, deviations, and
conformance checks. For find/check/explain operations, pass the repo-relative
path, symbol/member id, framework role, or pattern question you already have;
returned family ids are follow-up handles for exact `show_family` calls. If
RepoGrammar returns `UNKNOWN`, fallback, stale evidence, or omitted spans, use
normal source reads for the affected files.
