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

Agent wiring refreshes a managed instruction block only when the existing Codex
integration is safely owned **and** an explicit instruction-file override is
configured for that install path. First identify the actual global guide used
by your Codex installation or local policy. RepoGrammar does not discover or
guess that path. If you have verified that the common candidate below is your
active guide, inspect or refresh it explicitly without reconfiguring MCP or
touching repository state:

```text
repogrammar instructions status --file "$HOME/.codex/AGENTS.md" --json
repogrammar instructions sync --file "$HOME/.codex/AGENTS.md" --dry-run
repogrammar instructions sync --file "$HOME/.codex/AGENTS.md" --yes
```

Use the actual explicit path when `CODEX_HOME` or local policy places the guide
elsewhere. Sync creates or appends the exact managed block when it is absent,
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
