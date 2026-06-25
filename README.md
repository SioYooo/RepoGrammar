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
discovery, syntax-only code-unit extraction, syntax-origin framework-role fact
storage, CodeUnit-derived structural IR storage, SQLite generation-storage
wiring, a dependency-free TypeScript worker unavailable stub, and repository
guard checks.

It does not yet implement pattern mining, TypeScript compiler analysis, query
execution, or a working MCP server. The Rust-side TypeScript semantic-worker
adapter can execute a configured process, send the v1 request payload, and
validate NDJSON v1 facts. A checked-in worker stub can validate stdin and report
semantic analysis as unavailable. `index` and `sync` do not launch a worker by
default; when `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit executable, they
may run that worker with optional argv from
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` and store only facts that pass the
building generation's path/hash/range evidence gate. Stored semantic facts are
not family evidence and do not enable query or MCP claims. The storage/query
boundary can load an internal read-only claim-input snapshot from one active
generation, then run a file-hash freshness and readiness gate for future claim
builders; that gate does not classify families.
`init`, `uninit`, `unlock`, and `logs` operate only on safe repo-local lifecycle
state. `index` and `sync` now create a SQLite generation from TS/JS discovery
metadata plus syntax-only `code_units` records and structural IR records:
repo-relative path, language, kind, byte range, strict content hash, one IR node
per code unit, and conservative containment edges. They may also store
syntax-origin `FRAMEWORK_ROLE` facts with `FRAMEWORK_HEURISTIC` certainty for
recognized Express, React, and Jest/Vitest code-unit shapes, without launching a
semantic worker. They do not store source snippets, absolute paths, families, or
pattern-family evidence. `files` and `units` can read the active
file-manifest-only or syntax-only generation for inventory/debugging, but they
do not return pattern-family evidence. `status` and `doctor` can distinguish
file-manifest-only generations from syntax-only code-unit/IR generations.
Commands that install agent configuration or serve MCP return explicit
not-implemented errors until those contracts are implemented and tested.

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
cargo run --quiet --bin repogrammar -- family --project . --token-budget 8000 <family-id>
cargo run --quiet --bin repogrammar -- explain --project . --token-budget 8000 <target>
cargo run --quiet --bin repogrammar -- check --project . --token-budget 8000 <target>
```

The lifecycle surface is intentionally present before the full engine exists so
repo-local state boundaries, command contracts, tests, and documentation can
stabilize before mining begins. Pattern-family query commands currently return
explicit missing-index or missing-family-evidence fallback guidance; with
`--json`, that fallback is a structured object. `files` and `units` are limited
to active file-manifest-only or syntax-only index metadata.

## Product Shape

| Area | Current state | Target shape |
|---|---|---|
| Language scope | v0.1 contracts are TypeScript/JavaScript first | Production-quality TS/JS pattern-family evidence |
| Python | Planned second official language; pre-v0.2 work is experimental dogfooding only | Experimental FastAPI, pytest, SQLAlchemy, and Pydantic validation until a focused v0.2 adapter is accepted |
| Parsing | Dependency-free syntax-only TS/JS extractor stores structural code-unit candidates; Tree-sitter boundary remains planned | Tree-sitter generates syntax candidates, not final semantic truth |
| Semantics | Rust-side process adapter has request/output protocol fixtures and validates NDJSON v1 worker output; checked-in worker stub reports compiler analysis unavailable; `index`/`sync` can optionally run an explicit worker executable plus JSON argv vector and store only same-generation validated facts; default indexing can store syntax-origin framework-role facts with framework-heuristic certainty; compiler worker implementation and claims remain deferred | Language-native semantic workers provide compiler/API facts |
| Discovery | TS/JS discovery feeds syntax-only `index`/`sync` generations | Git-aware source inventory feeding parser and storage |
| Storage | SQLite generation schema, PRAGMAs, validation, activation pointer, indexed files, syntax-only code units, syntax-origin framework-role fact records, active file-manifest-only or syntax-only files/units read path, validated semantic-fact/evidence write/read substrate, internal active-generation claim-input snapshot, semantic-fact freshness/readiness gate, and status/doctor health reporting are implemented behind ports | Local evidence index wired to semantic workers, family read paths, migrations, and provenance |
| State directory | Safe `.repogrammar/` lifecycle plus file-manifest-only and syntax-only active generations are implemented | One repository-derived SQLite index per project, not a global code-derived database |
| MCP | Tool contracts are specified | Read-only agent tools backed by stored family evidence |
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
- keep TypeScript compiler API integration, freshness-validated semantic
  claims, mining, pattern-family query execution, and MCP transport deferred
  until parser output, storage, and semantic-worker boundaries are validated
  together.
- keep experimental Python dogfooding, optional CodeGraph provider work, and
  typed `UNKNOWN` governance explicitly scoped before implementation.

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
node src/workers/typescript/worker.test.js
cargo run --quiet --bin repo-guard -- check
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
