# Codex Quickstart

Install the CLI using the [general quickstart](quickstart.md#install), then
connect this agent and initialize each repository. Python 3.10+ is required.

## Preview And Apply Codex Wiring

```text
repogrammar install --target codex --scope global --dry-run --no-telemetry
repogrammar install --target codex --scope global --yes --no-telemetry
```

Then initialize each repository separately:

```text
cd /path/to/your/repo
repogrammar init
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

For installation, updates, and cleanup, return to the [general quickstart](quickstart.md).
