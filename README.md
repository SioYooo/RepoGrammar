# RepoGrammar

Local implementation-pattern intelligence for coding agents.

RepoGrammar is a pre-alpha local analysis engine for discovering recurring
repository implementation pattern families and returning auditable source
evidence. It is designed for agents and maintainers who need to know how a
codebase usually implements something before they change it.

RepoGrammar is not a call-graph explorer. The core product shape is
pattern-family evidence: dominant conventions, accepted variations, exceptions,
counterexamples, and `UNKNOWN` when static analysis cannot justify a stronger
claim.

## Status

This repository is in bootstrap state. It currently contains governance,
documentation, CI, a Rust core skeleton, semantic-worker boundaries, a
pattern-family-first CLI boundary, repo-local lifecycle commands, TS/JS file
discovery, Python `.py` discovery, syntax-only code-unit extraction,
syntax-origin framework-role fact storage, CodeUnit-derived structural IR
storage, SQLite generation-storage wiring, a dependency-free TypeScript worker
unavailable stub, a CPython AST-backed Python worker for structural code units,
stored internal structural anchors, and typed dynamic/unresolved `UNKNOWN`
output,
repository guard checks, and a read-only MCP `repogrammar_context` stdio
boundary.

The official v0.1 implementation target has pivoted to Python-first analysis
for FastAPI, pytest, SQLAlchemy, and Pydantic. The current TS/JS code remains
transitional substrate from the earlier bootstrap while Python discovery,
parser, framework-role, import-resolution, and family-evidence work is
implemented. The first Python slice discovers `.py` files, extracts CPython
`ast` structural code units for FastAPI route-shaped functions, pytest
tests/fixtures, Pydantic models, and SQLAlchemy model/repository-shaped units,
emits worker-local structural anchors for imports/decorators/class bases/simple
calls/test and fixture dependency edges, stores those anchors as internal parser-origin
`STRUCTURAL`/`UNKNOWN` facts, and stores framework-role heuristic facts without
turning raw facts into family claims. A bounded application-layer derivation
step can now synthesize separate `DATAFLOW_DERIVED` support facts only when
validated CPython anchors exact-match the Python framework compatibility table
for a unit with one framework role and no claim-relevant parser-origin blocking
`UNKNOWN`. This is sound-by-abstention bounded Python framework-family claims,
not sound Python semantic analysis; low-support and dynamic cases still return
typed `UNKNOWN`. Ready Python exact-anchor families pass bounded complete-link
clustering over support-family features before being stored, so bridge examples
cannot single-link incompatible Python support into one confident claim. They
may record metadata-only variation evidence when exact-compatible
framework-anchor support targets differ. Exact-anchor derivation now uses only
top-level framework bindings that have not been shadowed, and module-scope
dynamic import/path mutation becomes unit-scoped blocking `UNKNOWN` evidence
instead of being guessed away. The Python plan still uses a
claim-driven selective
cascade: cheap CPython syntax/scope/config facts first, Pyrefly only for
plausible family candidates, Pyright only for claim-upgrading cross-checks, and
typed `UNKNOWN` when evidence cannot support a claim.

It does not yet implement TypeScript compiler analysis, broad installer writes,
or full EC-MVFI mining. The Rust-side TypeScript semantic-worker adapter
can execute a configured process, send the v1 request payload, and validate
NDJSON v1 facts. A checked-in worker stub can validate stdin and report semantic
analysis as unavailable. `index` and `sync` do not launch a worker by default;
when `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit executable, they may run
that worker with optional argv from
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` and store only facts that pass the
building generation's path/hash/range evidence gate. The current EC-MVFI-lite
builder may write family records only when compatible framework-role candidates
also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED` support;
syntax-only framework heuristics remain insufficient support.
`init`, `uninit`, `unlock`, and `logs` operate only on safe repo-local lifecycle
state. `index` and `sync` now create a SQLite generation from TS/JS discovery
metadata plus syntax-only `code_units` records and structural IR records:
repo-relative path, language, kind, byte range, strict content hash, one IR node
per code unit, and conservative containment edges. They may also store
syntax-origin `FRAMEWORK_ROLE` facts with `FRAMEWORK_HEURISTIC` certainty for
recognized Express, React, and Jest/Vitest code-unit shapes, without launching a
semantic worker. The Python path may also store root `pyproject.toml` as a
`python-config` inventory file and `project_config` unit with sanitized
`PROJECT_CONFIG`/`STRUCTURAL` metadata or typed config `UNKNOWN`s. It may write
Python family records for exact-anchor derived support when the current
EC-MVFI-lite gate has enough compatible support, but it does not store source
snippets or absolute paths and does not claim provider-backed Python semantics.
`files` and `units` can read the active file-manifest-only or
syntax-only generation for inventory/debugging. Pattern
family query commands are wired to the active FamilyStore read path: with no
active index they still return fallback guidance, and with an active index but
insufficient family evidence they return typed `UNKNOWN` instead of pretending a
family was proven. `status` and `doctor` can distinguish file-manifest-only
generations from syntax-only code-unit/IR generations. `serve` exposes the same
conservative query path through the single default MCP tool
`repogrammar_context`. `install` and `uninstall` support narrow explicit-target
Codex/Claude Code MCP configuration through native agent CLIs after a read-only
MCP self-test; broad `--target all`, unsupported native scopes, executable
copying, and instruction-file edits remain deferred.

## Why RepoGrammar?

Coding agents usually learn repository conventions the slow way: searching
files, reading examples one by one, and guessing which examples are canonical.
That is brittle when a repository has multiple styles, legacy exceptions, test
helpers, framework conventions, or partial migrations.

RepoGrammar's goal is to answer a different question:

> What implementation family does this target belong to, what variations are
> legitimate here, and what source evidence supports that conclusion?

The intended output is a small, auditable evidence set rather than a long list of
similar files. A strong result should distinguish:

- `DOMINANT_PATTERN`: the high-support repository convention.
- `VARIATION`: an accepted local variation slot.
- `EXCEPTION`: a real counterexample or legacy/special-case implementation.
- `UNKNOWN`: insufficient static evidence, competing families, dynamic behavior,
  or an unsupported target.

## Get Started

RepoGrammar is currently a Rust workspace, not an installable production tool.
Use Cargo from the repository root:

```text
cargo run --quiet --bin repogrammar -- version
cargo run --quiet --bin repogrammar -- help
cargo run --quiet --bin repogrammar -- init
cargo run --quiet --bin repogrammar -- index
cargo run --quiet --bin repogrammar -- files --json
cargo run --quiet --bin repogrammar -- units --json
cargo run --quiet --bin repogrammar -- status
cargo run --quiet --bin repogrammar -- doctor --json
```

Try the current pattern-family CLI boundary:

```text
cargo run --quiet --bin repogrammar -- find --project . --token-budget 8000 <target>
cargo run --quiet --bin repogrammar -- family --project . --mode compact <family-id>
cargo run --quiet --bin repogrammar -- family --project . --mode evidence --token-budget 8000 <family-id>
cargo run --quiet --bin repogrammar -- explain --project . --token-budget 8000 <target>
cargo run --quiet --bin repogrammar -- check --project . --token-budget 8000 <target>
```

The lifecycle surface is intentionally present before the full engine exists so
repo-local state boundaries, command contracts, tests, and documentation can
stabilize before full mining begins. Pattern-family query commands currently
return explicit missing-index fallback guidance before `index`; after an active
index exists, they return typed `UNKNOWN` when family evidence is insufficient.
With `--json`, both states are parseable. Matched family detail defaults to
compact output without evidence records; explicit `--mode evidence` or
`--mode deep` returns selected repo-relative evidence metadata only, with
coverage labels and missing requested variation/exception coverage reported
without source snippets. `files` and `units` are limited to active
file-manifest-only or syntax-only index metadata.

## Product Shape

| Area | Current state | Target shape |
|---|---|---|
| Language scope | v0.1 target is Python-first; current code still contains TS/JS bootstrap substrate | Production-quality Python pattern-family evidence for FastAPI, pytest, SQLAlchemy, and Pydantic |
| TS/JS | Transitional substrate from the earlier bootstrap | Deferred production-quality TS/JS pattern-family evidence after Python v0.1 unless a later ADR changes scope |
| Parsing | Dependency-free syntax-only TS/JS extractor and CPython `ast`/`symtable`-backed Python extractor store structural code-unit, module, scope, and source-tied repo-local import candidates; private `tomllib` config summaries, semantic-worker-compatible project-mode repo-local module import facts, and Python release fixtures smoke this path without creating claims by default | Public parser-backed syntax candidates, not final semantic truth |
| Semantics | Rust-side process adapter has request/output protocol fixtures and validates NDJSON v1 worker output; checked-in worker stub reports compiler analysis unavailable; `index`/`sync` can optionally run an explicit worker executable plus JSON argv vector and store only same-generation validated facts; default indexing can store syntax-origin framework-role facts with framework-heuristic certainty and separate exact-anchor Python `DATAFLOW_DERIVED` support facts; compiler/provider worker implementation remains deferred | Language-native semantic workers provide compiler/API facts |
| Discovery | TS/JS and `.py` discovery feed syntax-only `index`/`sync` generations; Python virtualenv/cache/dependency dirs are skipped | Git-aware Python source inventory plus package/import context feeding parser and storage |
| Storage | SQLite generation schema, PRAGMAs, validation, activation pointer, indexed files, syntax-only code units, syntax-origin framework-role fact records, active files/units and family read paths, validated semantic-fact/evidence write/read substrate, EC-MVFI-lite family claim storage when strong semantic/dataflow support exists, Python fixture smoke for stale evidence, and status/doctor health reporting are implemented behind ports | Local evidence index wired to semantic workers, richer family read paths, migrations, and provenance |
| State directory | Safe `.repogrammar/` lifecycle plus file-manifest-only and syntax-only active generations are implemented | One repository-derived SQLite index per project, not a global code-derived database |
| MCP | Read-only `repogrammar_context` serve boundary is implemented | Stable agent-tool API after more compatibility testing |
| Telemetry | Consent boundaries are specified | Anonymous telemetry separate from research traces, disabled by default |
| Optional providers | No provider dependency | CodeGraph may be considered only as an optional lower-layer evidence provider, not a required runtime |

RepoGrammar does not depend on cloud services, local LLMs, embedding models,
vector databases, or remote APIs for v0.1.

Repository-derived analysis state belongs in `.repogrammar/` by default. Global
state is limited to installation receipts, binary/cache metadata, telemetry
preference, anonymous machine id, and non-repository-derived runtime artifacts.

## Evidence Discipline

RepoGrammar must not turn weak static hints into confident claims. Structural
similarity can generate candidates, but it cannot prove semantic family
membership by itself.

Primary evidence should come from repository-local source, normalized structure,
language-native semantic facts, framework-aware adapters, explicit provenance,
and contrastive examples. When evidence is ambiguous, stale, unsupported, or
outside the official language scope, RepoGrammar must abstain with `UNKNOWN`.

## CLI Reference

The v0.1 CLI is organized around implementation-pattern families:

```text
repogrammar init
repogrammar uninit
repogrammar index
repogrammar sync
repogrammar status
repogrammar doctor
repogrammar unlock
repogrammar logs
repogrammar find
repogrammar families
repogrammar family
repogrammar member
repogrammar explain
repogrammar check
repogrammar files
repogrammar units
repogrammar serve
repogrammar install
repogrammar uninstall
repogrammar stats
repogrammar telemetry
repogrammar version
repogrammar help
```

The following graph-navigation names are intentionally not top-level v0.1
commands: `callers`, `callees`, `impact`, `affected`, `node`, and `explore`.
If call-graph functionality is added later, it must remain secondary to the
pattern-family product shape.

See [docs/specifications/cli.md](docs/specifications/cli.md) for the command
contract.

## Architecture

RepoGrammar uses a Rust primary core with room for language-native semantic
workers under `src/`:

```text
src/rust: Rust core, analysis engine, CLI, MCP, storage, repository guard
src/workers: language-native semantic worker entries and future compiler integrations
src/protocol: versioned worker protocol documents and schemas
```

Reference metadata that is not executable source lives outside `src/`:

```text
algorithms/paper: metadata-only algorithm and supply-chain reference archive
```

The dependency direction and module ownership are documented in:

- [docs/architecture/overview.md](docs/architecture/overview.md)
- [docs/architecture/module-map.md](docs/architecture/module-map.md)
- [docs/architecture/dependency-rules.md](docs/architecture/dependency-rules.md)

## Roadmap

The next implementation phase should refine one boundary at a time from the
v0.1 parallel development plan:

- keep syntax-only code units structural and non-semantic;
- keep syntax-origin framework-role facts framework-heuristic and out of family
  claims until stronger evidence and claim builders exist;
- pivot the next analysis work to Python discovery, parser boundaries,
  repo-local import resolution, framework-role extraction, and Python
  EC-MVFI-lite fixtures;
- keep TypeScript compiler API integration, full mining, broad installer writes,
  and instruction-file integration deferred until parser output, storage,
  semantic-worker, MCP self-test, and receipt boundaries are validated together;
- keep optional CodeGraph provider work and typed `UNKNOWN` governance
  explicitly scoped before implementation.

See [docs/roadmap.md](docs/roadmap.md) and
[docs/plans/v0.1-parallel-development-plan.md](docs/plans/v0.1-parallel-development-plan.md)
for the staged plan.

## Development

Requirements:

- Rust stable toolchain from [rust-toolchain.toml](rust-toolchain.toml)
- Cargo with rustfmt and clippy components
- Node.js for the dependency-free TypeScript worker stub smoke test
- Git for diff-based repository guard checks

Required verification:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD
git diff --check origin/main...HEAD
cmp -s AGENTS.md CLAUDE.md
```

Repository documentation starts at [docs/README.md](docs/README.md). The mirrored
agent contract lives in [AGENTS.md](AGENTS.md) and [CLAUDE.md](CLAUDE.md), which
must remain byte-identical.

## Star History

<a href="https://www.star-history.com/?repos=SioYooo%2FRepoGrammar&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=SioYooo/RepoGrammar&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=SioYooo/RepoGrammar&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=SioYooo/RepoGrammar&type=date&legend=top-left" />
 </picture>
</a>

## License
