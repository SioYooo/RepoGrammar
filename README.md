# RepoGrammar

Local implementation-pattern context for coding agents.

RepoGrammar helps coding agents and maintainers understand how a repository
usually implements a feature before changing it. Instead of returning a long
list of vaguely similar files, it tries to summarize recurring implementation
families, accepted variations, exceptions, and the evidence behind those
claims.

RepoGrammar is **pre-alpha**. The current public preview is local-first,
metadata-only, and conservative: when evidence is insufficient, stale,
ambiguous, or outside the supported scope, RepoGrammar returns `UNKNOWN`
instead of guessing.

## What It Does

RepoGrammar is designed to answer questions like:

- What local implementation pattern does this target resemble?
- Which examples are canonical, and which are variations or exceptions?
- What files should an agent read before editing?
- Is this target likely to conform to a known local family?

The output is meant to be small and auditable. Current family results expose
metadata such as repo-relative paths, content hashes, byte ranges, support
counts, variation labels, and `UNKNOWN` reasons. Source snippets are not
returned by default.

## Current Scope

The scoped v0.1 preview focuses on Python repositories using:

- FastAPI
- pytest
- Pydantic
- SQLAlchemy

Current Python claims are bounded framework-family claims, not full Python
semantic analysis. RepoGrammar requires compatible repeated evidence before it
emits a confident family claim. Low-support, dynamic, stale, or unresolved cases
remain `UNKNOWN`.

TypeScript and JavaScript are not official v0.1 support targets.

## Install

For public-preview releases, install the prebuilt CLI binary first:

```text
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/latest/download/install.sh
bash install.sh
```

The installer downloads the matching macOS or Linux release artifact, verifies
its checksum, installs `repogrammar` into a user-writable command directory, and
then can launch the agent setup wizard. It does not require Rust, Cargo,
Node.js, Docker, a local LLM, an embedding model, or a cloud API key. The
current Python preview still requires a `python3` interpreter at indexing time
to run RepoGrammar's bundled CPython AST worker; it does not require a Python
virtualenv or project dependency installation.

On Windows preview builds, use PowerShell:

```text
Invoke-WebRequest https://github.com/SioYooo/RepoGrammar/releases/latest/download/install.ps1 -OutFile install.ps1
powershell -ExecutionPolicy Bypass -File install.ps1
```

Agent integration requires the native CLI for the agent you choose: `codex` for
Codex integration and `claude` for Claude Code integration. Missing agent CLIs
are non-fatal; you can configure the agents that are available and rerun
`repogrammar install` later.

From a source checkout, the same installer lives at:

```text
bash src/install/repogrammar-install.sh
```

That script can install or repair the `repogrammar` command from a release,
configure Codex and Claude Code, uninstall connected agent integrations, remove
the local `repogrammar` command, or, for contributors, build from source.

You can also review the plan without writing anything:

```text
repogrammar install --target all --scope global --dry-run --no-telemetry
```

The installer configures machine-level coding-agent MCP integration. It does not
index the current repository and does not create or modify `.repogrammar/`.

Supported public-preview binary targets are macOS arm64/x86_64, Linux
arm64/x86_64, and Windows x86_64 preview. Release artifacts include the
RepoGrammar binary and the Python worker asset used by the current Python
frontend.

## Quick Start

From a repository you want to analyze:

```text
repogrammar install
repogrammar init
repogrammar index
repogrammar status
repogrammar families
repogrammar find --project . --token-budget 8000 <target>
repogrammar family --project . --mode compact <family-id>
repogrammar explain --project . --token-budget 8000 <target>
repogrammar check --project . --token-budget 8000 <target>
```

Before a repository is initialized or before enough evidence exists, query
commands return explicit fallback or `UNKNOWN` results rather than pretending an
index or family claim exists.

## Agent Integration

RepoGrammar exposes a read-only MCP tool named `repogrammar_context`.

Global MCP integration is supported for Codex and Claude Code. Running
`repogrammar install` with no flags opens a simple text wizard that lets you
select Codex, Claude Code, or both in one run. Re-running the wizard later lets
you add a missing supported agent or repair the `repogrammar` command without
reinstalling already managed agents.

Noninteractive dry-runs remain available:

```text
repogrammar install --target all --scope global --dry-run --no-telemetry
```

After reviewing the plan, use `--yes` for explicit noninteractive installs:

```text
repogrammar install --target codex --scope global --yes --no-telemetry
repogrammar install --target claude-code --scope global --yes --no-telemetry
repogrammar install --target all --scope global --yes --no-telemetry
```

Multi-agent install is all-or-rollback: if one selected agent fails, changes
from the same run are rolled back. Project-local live writes are deferred.

## Privacy And Telemetry

RepoGrammar is local-first. Repository-derived indexes live under
`.repogrammar/` in the analyzed project.

Anonymous telemetry is off by default. `install --yes` does not imply telemetry
consent. Telemetry, when explicitly enabled, is designed to use coarse
allowlisted product metrics and must not include source code, prompts, query
text, repository names, paths, symbols, credentials, or raw errors.

Useful commands:

```text
repogrammar telemetry status --json
repogrammar telemetry on
repogrammar telemetry off
repogrammar telemetry export --json
repogrammar telemetry purge --yes
```

## Limitations

RepoGrammar is not production-ready and should not be treated as a sound static
analyzer. In this preview:

- Python support is limited to bounded framework-family evidence.
- Dynamic Python behavior often produces typed `UNKNOWN`.
- Source snippets are not returned by default.
- Full Python semantic providers, runtime observation, and broader language
  support are deferred.
- Telemetry upload remains experimental and opt-in.

## Documentation

- [Documentation map](docs/README.md)
- [CLI specification](docs/specifications/cli.md)
- [Product specification](docs/specifications/product.md)
- [Python analysis specification](docs/specifications/python-analysis.md)
- [MCP API specification](docs/specifications/mcp-api.md)
- [Telemetry specification](docs/specifications/telemetry.md)
- [Roadmap](docs/roadmap.md)

## Development

Contributors building from source need Rust/Cargo. Node.js is needed only for
TypeScript worker tests; Python 3 is needed for Python indexing and Python
worker tests. For contributor setup, architecture, and validation commands,
start with [docs/README.md](docs/README.md) and
[docs/development/testing.md](docs/development/testing.md).

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=SioYooo/RepoGrammar&type=Date)](https://www.star-history.com/#SioYooo/RepoGrammar&Date)

## License

MIT
