# RepoGrammar

Local implementation-pattern context for coding agents.

RepoGrammar helps coding agents and maintainers understand how a repository
usually implements a feature before changing it. Instead of returning a long
list of vaguely similar files, it tries to summarize recurring implementation
families, accepted variations, exceptions, and the evidence behind those
claims.

RepoGrammar is **pre-alpha**. The current public preview is local-first and
conservative: when evidence is insufficient, stale, ambiguous, or outside the
supported scope, RepoGrammar returns `UNKNOWN` instead of guessing.

## What It Does

RepoGrammar is designed to answer questions like:

- What local implementation pattern does this target resemble?
- Which examples are canonical, and which are variations or exceptions?
- What files should an agent read before editing?
- Is this target likely to conform to a known local family?

The output is meant to be small and auditable. Current family results expose
metadata such as repo-relative paths, content hashes, byte ranges, support
counts, line ranges when source hashes are fresh, variation labels, and
`UNKNOWN` reasons. Source snippets are not returned by default. When explicitly
requested, RepoGrammar can return bounded, line-numbered source spans selected
from its hash-checked read plan.

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

TypeScript and JavaScript are not official v0.1 support targets. A conservative
v0.2 token-saving foundation adds exact-anchor family support for Express
routes, Jest/Vitest suites/tests, and structural-preview Next.js, Fastify,
Prisma, and Drizzle adapters: a family is claimed solely when an exact local
anchor is promoted to application-layer `DATAFLOW_DERIVED` support. Lookalikes,
package-only evidence, reassigned or shadowed bindings, dynamic receivers or
methods, custom test wrappers, React components/hooks, Next middleware/server
actions/re-exports, Fastify plugin prefixes, Prisma bulk/injected/raw clients,
and Drizzle raw/dynamic builders stay `UNKNOWN`. Next dynamic segments, route
groups, and parallel routes are retained as bounded context on exact local
page/layout/route anchors rather than treated as membership proof by
themselves. TS/JS family claims require at least three compatible
exact-anchor support facts and use conservative complete-link clustering so
incompatible handler/test/component/query shapes do not single-link into one
family. Bounded `package.json`, `tsconfig.json`, `jsconfig.json`, and
Jest/Vitest config inventory is stored as structural context only. A bounded
repo-local static import resolver can record relative/path-alias imports as
structural context, while dynamic imports, conditional `require`, unresolved
aliases, and star re-exports remain typed `UNKNOWN`. This is not full
TypeScript/JavaScript semantic analysis.
For Jest/Vitest, package dependencies and JSON config files can provide ambient
runner context; script configs such as `jest.config.ts` or `vitest.config.js`
are recorded only as metadata/typed `UNKNOWN` and are not executed.

Rust is currently used only for RepoGrammar self-dogfooding. The v0.2 preview
can index `.rs` files with Tree-sitter Rust and form conservative internal
RepoGrammar implementation families such as indexing phases, family gates,
parser adapters, CLI/MCP handlers, installer actions, storage validation, and
product tests. This path is structural only: it does not run Cargo, rustc,
build scripts, or procedural macros. Cargo build scripts and target-specific
manifest sections are typed build-variant `UNKNOWN`s that block affected Rust
self-dogfood families until resolved; this is not general-purpose Rust semantic
analysis.

## Public Preview Support Matrix

| Area | Public-preview status | Boundary |
|---|---|---|
| Python FastAPI | Supported | Bounded exact-anchor framework-family evidence only; dynamic decorators, unresolved imports, runtime DI, and stale evidence produce `UNKNOWN`. |
| Python pytest | Supported | Bounded test/fixture family evidence; ambiguous fixture injection and dynamic plugin behavior produce `UNKNOWN`. |
| Python Pydantic | Supported | Bounded model/settings family evidence; dynamic model factories and unresolved bases produce `UNKNOWN`. |
| Python SQLAlchemy | Supported | Bounded SQLAlchemy model/repository evidence; dynamic declarative behavior remains conservative. |
| JS/TS Express | v0.2 conservative preview | Exact import/require binding plus direct literal route calls only; support requires at least three complete-link-compatible examples. |
| JS/TS Jest/Vitest | v0.2 conservative preview | Imported runner, imported alias runner, or ambient test-file runner with safe project context only; custom wrappers and foreign runner imports stay `UNKNOWN`. |
| JS/TS Next.js | v0.2 structural preview | `next` package context plus exact local App Router pages/layouts/routes and Pages Router pages/API routes; dynamic segments, route groups, and parallel routes are context assumptions on exact anchors, while middleware, server/client semantics, server actions, and re-exported routes remain `UNKNOWN`. |
| JS/TS Fastify | v0.2 structural preview | Exact local Fastify factory receiver plus shorthand or literal `app.route` declarations; dynamic methods/options, plugin registration, and prefix semantics remain `UNKNOWN`. |
| JS/TS Prisma | v0.2 structural preview | Exact local `new PrismaClient()` bindings plus whitelisted model read/write operations and array `$transaction`; bulk operations, injected clients, extensions, callback transactions, dynamic model/op access, and raw SQL remain `UNKNOWN`. |
| JS/TS Drizzle | v0.2 structural preview | Exact Drizzle table factories, local `drizzle(...)` db bindings, whitelisted `select`/`insert`/`update`/`delete`, and `db.query.<table>.findMany/findFirst`; unresolved tables/dbs, dynamic builders, and raw SQL remain `UNKNOWN`. |
| JS/TS React | Not supported | Components/hooks may be detected as roles but cannot form public family claims. |
| Full JS/TS semantic analysis | Not supported | No compiler-backed TypeScript resolution, full alias/re-export semantics, dynamic wrapper support, or project execution. |
| Rust self-dogfood | Internal v0.2 preview | RepoGrammar-owned implementation families only, from Tree-sitter structural anchors; no Cargo/rustc/proc-macro execution and no general Rust semantic claims. |
| Source text output | Explicit opt-in only | Default CLI/MCP output is metadata-only; `--include-source-spans` / `include_source_spans=true` returns bounded hash-checked line-numbered spans. |
| Token savings | Not claimed by default | Token-saving claims require paired baseline/treatment measurements. `estimated_potential_token_savings` is a local ESTIMATED potential-read-displacement diagnostic, not measured savings. |
| Project-local live install | Deferred | Public preview live writes are machine-level agent wiring only; per-repository `.repogrammar/` lifecycle uses explicit `init`/`resync`/`autosync` commands. |

## Install

Once a public-preview prerelease has been published, install the prebuilt CLI
binary first. During preview, use the exact prerelease tag rather than GitHub's
`latest` redirect:

```text
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.2.0-preview.0/install.sh
bash install.sh --version v0.2.0-preview.0
```

After the npm package is published, users with Node/npm can use the wrapper to
download the same prebuilt binary and run it through `npx`:

```text
npx @sioyooo/repogrammar install
```

After publication, you can also install the wrapper globally:

```text
npm install -g @sioyooo/repogrammar
repogrammar install
```

The installer downloads the matching macOS/Linux release artifact, verifies its
checksum, installs `repogrammar` into a user-writable command directory, and
then can launch the agent setup wizard. Windows preview builds use the
PowerShell installer below. Neither path requires Rust, Cargo, Node.js, Docker,
a local LLM, an embedding model, or a cloud API key. The current Python preview
still requires a `python3` interpreter at indexing time to run RepoGrammar's
bundled CPython AST worker; it does not require a Python virtualenv or project
dependency installation.

The `npx` / npm path requires Node/npm by definition, but still does not require
Rust or Cargo. On Windows preview builds, use PowerShell:

```text
Invoke-WebRequest https://github.com/SioYooo/RepoGrammar/releases/download/v0.2.0-preview.0/install.ps1 -OutFile install.ps1
powershell -ExecutionPolicy Bypass -File install.ps1 -Version v0.2.0-preview.0
```

Agent integration requires the native CLI for the agent you choose: `codex` for
Codex integration and `claude` for Claude Code integration. Missing agent CLIs
are non-fatal; you can configure the agents that are available and rerun
`repogrammar install` later.

The installer separates three steps: get the CLI, wire coding agents, then run
`repogrammar init` / `repogrammar index` inside each repository. Agent wiring
does not index code and does not create `.repogrammar/`.

From a source checkout, the same installer lives at:

```text
bash src/install/repogrammar-install.sh
powershell -ExecutionPolicy Bypass -File src/install/install.ps1
```

Those scripts can install or repair the `repogrammar` command from a release,
configure Codex and Claude Code, uninstall connected agent integrations, remove
the local `repogrammar` command, or, for contributors, build from source. On
Windows, the PowerShell source-checkout menu defaults to the same contributor
source-build path. Until the first prerelease artifact exists, source-checkout
dogfood is the supported path:

```text
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
bash src/install/repogrammar-install.sh --install-and-configure --from-source --yes --target all
powershell -ExecutionPolicy Bypass -File src/install/install.ps1 -InstallCliOnly -FromSource -Yes
powershell -ExecutionPolicy Bypass -File src/install/install.ps1 -InstallAndConfigure -FromSource -Yes -Target all
```

The source path installs the built binary into RepoGrammar-managed user state
and refreshes the user-writable `repogrammar` command without requiring a
GitHub Release asset or published npm package. If that command path already
contains an older unmanaged `repogrammar`, the installer backs it up before
replacing it with the managed command. After install/update, the scripts compare
other `repogrammar` copies on PATH by SHA256 and remove stale copies when
`--yes` / `-Yes` is used; on Windows, `install.ps1 -Verify` reports the same
state and `install.ps1 -Prune -Yes` runs the cleanup explicitly. If a stale
copy cannot be removed because a process or permissions blocks deletion, the
installer exits nonzero and leaves the stale copy listed for manual cleanup.

Before the npm package is published, local npm dogfood can bypass release
downloads with an already built binary:

```text
REPOGRAMMAR_BINARY=/absolute/path/to/target/release/repogrammar node src/npm/repogrammar.js version
npm pack
npm install -g ./sioyooo-repogrammar-0.1.0.tgz
```

The published npm path still expects release artifacts by default.

You can also review the plan without writing anything:

```text
repogrammar install --target all --scope global --dry-run --no-telemetry
```

The installer configures machine-level coding-agent MCP integration. It does not
index the current repository and does not create or modify `.repogrammar/`.

Supported public-preview binary targets are macOS arm64/x86_64, Linux
arm64/x86_64, and Windows x86_64 preview. Release artifacts include the
RepoGrammar binary and the Python worker asset used by the current Python
frontend. The npm wrapper is a thin launcher for those same release artifacts;
it is not the implementation of RepoGrammar.

## Quick Start

From a repository you want to analyze:

```text
repogrammar install
repogrammar init --yes --resync --autosync
repogrammar status
repogrammar autosync status
repogrammar prune --dry-run
repogrammar compact --dry-run --json
repogrammar logs --component daemon --tail 20
repogrammar families
repogrammar find --project . --token-budget 8000 <target>
repogrammar family --project . --mode compact <family-id>
repogrammar explain --project . --token-budget 8000 <target>
repogrammar check --project . --token-budget 8000 <target>
```

Before a repository is initialized or before enough evidence exists, query
commands return explicit fallback or `UNKNOWN` results rather than pretending an
index or family claim exists.

`repogrammar install` wires machine-level agent MCP integration only.
Repository analysis is always an explicit per-repository step:
`repogrammar init --yes --resync --autosync` creates safe repo-local state,
rebuilds the active static-analysis generation, and starts repository-local
auto-sync in one explicit command. `resync` shows progress automatically in an
interactive terminal. Use
`repogrammar resync --progress always` to force progress output, or
`repogrammar resync --progress never` for quiet scripts.

`repogrammar autosync start` is optional but recommended for dogfooding. It
enables repository-local auto-sync and starts a background worker that tracks a
lightweight supported-file metadata fingerprint, debounces saves, and reuses
the existing `sync` path so newly added or modified files enter the next active
generation without a manual `resync`. It does not run from `install`, does not
initialize other repositories, and can be inspected or managed with:

```text
repogrammar autosync status
repogrammar logs --component daemon --tail 20
repogrammar autosync stop
repogrammar autosync disable
```

## Agent Integration

RepoGrammar exposes a read-only MCP tool named `repogrammar_context`.

Global MCP integration is supported for Codex and Claude Code. Running
`repogrammar install` with no flags opens a simple text wizard that lets you
select Codex, Claude Code, or both in one run. Re-running the wizard later lets
you add a missing supported agent or repair the `repogrammar` command without
reinstalling already managed agents. Anonymous telemetry remains opt-in and
default-no in the wizard; after you review the install plan, pressing Enter at
the final confirmation proceeds.

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

For planning and manual configuration, the CLI also understands CodeGraph-style
target ids such as `cursor`, `opencode`, `hermes`, `gemini`, `antigravity`, and
`kiro` in dry-run and `--print-config` modes:

```text
repogrammar install --print-config cursor --location local
repogrammar install --target cursor,gemini --location local --dry-run --no-telemetry
```

Those preview targets are not live-written by the public preview installer yet.

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

Local paired token experiments can record already-redacted host usage counts
without manual token flags:

```text
repogrammar telemetry experiment-record --name <id> --usage-json usage.json
```

The usage file must contain only token counts plus optional success/test
outcome metadata; raw prompts, source, paths, symbols, messages, patches, and
query text are rejected.

## Limitations

RepoGrammar is not production-ready and should not be treated as a sound static
analyzer. In this preview:

- Python support is limited to bounded framework-family evidence.
- Dynamic Python behavior often produces typed `UNKNOWN`.
- JS/TS preview support is limited to exact-anchor Express, Jest/Vitest,
  Next.js, Fastify, Prisma, and Drizzle families. Full JS/TS semantics, React,
  dynamic wrappers, executable Jest/Vitest/Next config semantics, Fastify plugin
  prefix resolution, Prisma/Drizzle runtime extensions, and complete
  alias/re-export resolution are deferred.
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

<a href="https://www.star-history.com/?repos=SioYooo%2FRepoGrammar&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=SioYooo/RepoGrammar&type=date&theme=dark&legend=top-left&v=3" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=SioYooo/RepoGrammar&type=date&legend=top-left&v=3" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=SioYooo/RepoGrammar&type=date&legend=top-left&v=3" />
 </picture>
</a>

## License

MIT
