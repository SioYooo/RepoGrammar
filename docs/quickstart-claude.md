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

## Verify The Global Claude Code Pre-flight

The managed agent guidance can be inspected or refreshed independently of MCP
wiring by selecting the actual guide explicitly:

```text
repogrammar instructions status --file "$HOME/.claude/CLAUDE.md" --json
repogrammar instructions sync --file "$HOME/.claude/CLAUDE.md" --dry-run
repogrammar instructions sync --file "$HOME/.claude/CLAUDE.md" --yes
```

The command updates only an exact current or known legacy RepoGrammar marker
block, preserves unrelated instructions, and refuses foreign or malformed
marker content. It does not create `.repogrammar/`, run setup, or mirror
`AGENTS.md`.

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
before CodeGraph or broad source reads when implementation, test, fix,
refactor, or diagnosis requires a repository-local contract/convention,
repeated implementation, framework role, or analogue comparison. This includes
schema, protocol, API, and prompt-output contract conformance or drift. For
find/check/explain operations, pass the repo-relative path, symbol/member id,
framework role, or code-work question you already have; returned family ids are
follow-up handles for exact `show_family` calls. If RepoGrammar returns
`UNKNOWN`, fallback, stale evidence, or omitted spans, state that reason and use
normal source reads for the affected files.

## Exact No-Build Path

After the exact npm version and matching GitHub asset pass the availability
gate in `quickstart.md`:

```text
npx @sioyooo/repogrammar@0.2.0 setup --project /path/to/your/repo --target claude-code
```

If either check fails, use the source acquisition path above.
