# Codex Quickstart

For published 0.5.0, install the CLI using the
[general quickstart](quickstart.md#install), then use the explicit agent and
repository steps below. Python 3.10+ is required for Python analysis.

## Preview And Apply Codex Wiring

```text
repogrammar install --target codex --scope global --dry-run --no-telemetry
repogrammar install --target codex --scope global --yes --no-telemetry
```

Then initialize each repository separately:

```text
cd /path/to/your/repo &&
repogrammar init &&
repogrammar status
```

Agent installation never creates `.repogrammar/`; repository `init` never
configures Codex. `init` starts repo-local autosync by default. Add
`--no-autosync --progress never` for a one-shot index.

## Verify The Global Codex Pre-flight

Current source builds use absolute `CODEX_HOME`, defaulting to
`$HOME/.codex` only when it is unset, and prefer an existing nonempty `AGENTS.override.md` over
`AGENTS.md`. `install --dry-run` shows the selected path; confirmed install
writes a short conditional guide or refreshes its safely owned section.
Use `--no-instructions` to register MCP without that write. An empty/relative
explicit `REPOGRAMMAR_INSTRUCTION_FILE_CODEX` override stays deferred.
Published 0.5.0 artifacts retain their shipped override-only wiring.

Inspect the actual selected path. These examples apply to the default base
file only when no override shadows it:

```text
repogrammar instructions status --file "$HOME/.codex/AGENTS.md" --json
repogrammar instructions sync --file "$HOME/.codex/AGENTS.md" --dry-run
repogrammar instructions sync --file "$HOME/.codex/AGENTS.md" --yes
```

Use the actual explicit path when `CODEX_HOME` or local policy places the guide
elsewhere. Explicit-file sync creates the full v3 repository gate when absent, keeps a
current short v4 global profile unchanged,
refreshes only an exact known legacy block, preserves unrelated instructions,
and refuses foreign or malformed marker content. It does not create
`.repogrammar/`, run setup, or mirror `CLAUDE.md`.

## Use RepoGrammar in Codex

Restart Codex and open it in the initialized repository.

Ask:

```text
How are API routes implemented in this repository?
```

Codex should call the read-only `repogrammar_context` MCP tool before CodeGraph
or broad source reads when an implementation, test, fix, refactor, or diagnosis
requires repository-local contract/convention, repeated implementation,
framework-role, or analogue evidence. This includes schema, protocol, API, and
prompt-output contract drift. RepoGrammar returns evidence, a read plan, and
typed uncertainty. `UNKNOWN`, fallback, stale evidence, or omitted spans mean
Codex must state the reason and use normal source reads for the affected files;
they must never be upgraded into a confident family claim.

RepoGrammar runs locally and does not require an OpenAI API key.

## One-command candidate (unreleased)

This replaces the separate installation, agent wiring and repository setup
steps above. After it succeeds, use the [verification checks](quickstart.md#verify-setup)
and restart your agent; do not repeat the manual setup.

Current source can combine acquisition, Codex configuration and repository
setup in one reviewed plan. This is **not available in published 0.5.0**; a
new immutable release must supply the `quickstart-v1` capability. With reviewed
local candidate archives and checksums, run from the intended Git repository:

```bash
REPOGRAMMAR_RELEASE_DIR=/absolute/path/to/candidate-artifacts \
  bash /absolute/path/to/RepoGrammar/src/install/repogrammar-install.sh \
  --quickstart --target codex
```

Add `--dry-run` to inspect without persistent writes, or explicit `--yes` for
noninteractive authorization. `--no-instructions` skips instruction updates;
`--no-autosync` skips the background daemon. Agent configuration is global;
project-local writes remain deferred. Read the
[candidate installation and trust guide](quickstart.md#one-command-candidate-unreleased)
before using an installer downloaded from a distribution channel.

The result distinguishes product MCP self-test, native integration and
repository readiness. Restart Codex afterward and verify that its session
loads `repogrammar_context`; a successful config write alone does not prove an
operational agent session. Foreign or drifted integrations are preserved.

For installation, updates, and cleanup, return to the [general quickstart](quickstart.md).
