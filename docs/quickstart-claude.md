# Claude Code Quickstart

Install the CLI using the [general quickstart](quickstart.md#install), then
connect this agent and initialize each repository. Python 3.10+ is required.

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

Current source builds use absolute `CLAUDE_CONFIG_DIR`, defaulting to
`$HOME/.claude/CLAUDE.md` only when it is unset. `install --dry-run` shows the actual path; confirmed
install writes a short conditional guide or refreshes its safely owned section.
Use `--no-instructions` to register MCP without that write. An empty/relative
explicit `REPOGRAMMAR_INSTRUCTION_FILE_CLAUDE_CODE` override stays deferred.
Published 0.5.0 artifacts retain their shipped override-only wiring.

Inspect the actual selected path; these commands assume the default profile:

```text
repogrammar instructions status --file "$HOME/.claude/CLAUDE.md" --json
repogrammar instructions sync --file "$HOME/.claude/CLAUDE.md" --dry-run
repogrammar instructions sync --file "$HOME/.claude/CLAUDE.md" --yes
```

Use a different explicit path when local policy places the guide elsewhere.
Explicit-file sync creates the full v3 repository gate when absent, keeps a
current short v4 global profile unchanged, refreshes
only an exact known legacy block, preserves unrelated instructions, and refuses
foreign or malformed marker content. It does not create `.repogrammar/`, run
setup, or mirror `AGENTS.md`.

## Initialize A Project

Run this inside each repository where you want Claude Code to use RepoGrammar:

```text
repogrammar init --yes
repogrammar status
```

`repogrammar init --yes` is the agent-safe noninteractive bootstrap and builds
the first active index, then starts repo-local auto-sync by default so Claude
Code edits enter later RepoGrammar queries. Use `--no-autosync` for a one-shot
index without a background daemon.

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

For installation, updates, and cleanup, return to the [general quickstart](quickstart.md).
