# opencode Quickstart

This source-checkout flow installs RepoGrammar and configures the global
opencode MCP integration. Use it whenever the exact-version availability gate
in `quickstart.md` does not pass or contributor dogfood is desired.

opencode integration is file-based: RepoGrammar writes the managed
`mcp.repogrammar` entry into `$XDG_CONFIG_HOME/opencode/opencode.json`
(default `$HOME/.config/opencode/opencode.json`) directly and never runs an
`opencode` CLI for wiring, probing, or removal.

## Install The Command

```text
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

The current Python analysis path requires Python 3.10 or newer as `python3`.

## Review And Apply opencode Wiring

Dry-run first:

```text
repogrammar install --target opencode --scope global --dry-run --no-telemetry
```

Inspect the exact entry without writing:

```text
repogrammar install --target opencode --scope global --print-config --no-telemetry
```

Apply the global opencode integration only after reviewing the plan:

```text
repogrammar install --target opencode --scope global --yes --no-telemetry
```

RepoGrammar refuses to touch a malformed config file, preserves every unknown
field, backs the previous bytes up before modifying a pre-existing file, and
verifies the written entry by reparsing it. Removal is the exact inverse:

```text
repogrammar disconnect --target opencode --scope global --yes
```

`disconnect` removes only the `mcp.repogrammar` key and deletes the config file
only when RepoGrammar created it during the install and it would otherwise be
empty.

## Verify The Global opencode Pre-flight

First identify the actual global guide used by your opencode installation or
local policy. RepoGrammar does not discover or guess that path. If you have
verified that the candidate below is your active guide, inspect or refresh it
independently of MCP wiring:

```text
repogrammar instructions status --file "$XDG_CONFIG_HOME/opencode/AGENTS.md" --json
repogrammar instructions sync --file "$XDG_CONFIG_HOME/opencode/AGENTS.md" --dry-run
repogrammar instructions sync --file "$XDG_CONFIG_HOME/opencode/AGENTS.md" --yes
```

Use a different explicit path when local policy places the guide elsewhere.
Sync creates or appends the exact managed block when it is absent, refreshes
only an exact known legacy block, preserves unrelated instructions, and refuses
foreign or malformed marker content. It does not create `.repogrammar/`, run
setup, or mirror `AGENTS.md`.

## Initialize A Project

Run this inside each repository where you want opencode to use RepoGrammar:

```text
repogrammar init --yes
repogrammar status
```

`repogrammar init --yes` is the agent-safe noninteractive bootstrap and builds
the first active index, then starts repo-local auto-sync by default so opencode
edits enter later RepoGrammar queries. Use `--no-autosync` for a one-shot index
without a background daemon.

After initialization, opencode should use the `repogrammar_context` MCP tool
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

After the exact npm version, complete npm channel mapping, and matching GitHub
asset pass the availability gate in `quickstart.md`:

```text
npx --yes --package @sioyooo/repogrammar@0.4.3 \
  repogrammar install --target opencode --scope global --yes --no-telemetry
npx --yes --package @sioyooo/repogrammar@0.4.3 \
  repogrammar init --project /path/to/your/repo --yes
```

If any check fails, use the source acquisition path above.
