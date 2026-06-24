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
pattern-family-first CLI boundary, and repository guard checks.

It does not yet implement real pattern mining, indexing, storage migrations, or
a working MCP server. Commands that would mutate repositories, install agent
configuration, run indexing, or serve MCP return explicit not-implemented
errors until those contracts are implemented and tested.

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
cargo run --quiet --bin repogrammar -- status
cargo run --quiet --bin repogrammar -- doctor
```

Try the current pattern-family CLI boundary:

```text
cargo run --quiet --bin repogrammar -- find --project . --token-budget 8000 <target>
cargo run --quiet --bin repogrammar -- family --project . --token-budget 8000 <family-id>
cargo run --quiet --bin repogrammar -- explain --project . --token-budget 8000 <target>
cargo run --quiet --bin repogrammar -- check --project . --token-budget 8000 <target>
```

The command surface is intentionally present before the full engine exists so
contracts, tests, and documentation can stabilize around pattern-family results.

## Product Shape

| Area | Current state | Target shape |
|---|---|---|
| Language scope | v0.1 contracts are TypeScript/JavaScript first | Production-quality TS/JS pattern-family evidence |
| Python | Planned second official language | Experimental only until a focused v0.2 adapter is accepted |
| Parsing | Tree-sitter boundary is planned | Tree-sitter generates syntax candidates, not final semantic truth |
| Semantics | Worker boundary, v1 protocol tokens, schemas, and fixtures exist | Language-native semantic workers provide compiler/API facts |
| Storage | SQLite and FTS5 are specified | Local evidence index with migrations and provenance |
| State directory | Repo-local `.repogrammar/` is specified | One repository-derived SQLite index per project, not a global code-derived database |
| MCP | Tool contracts are specified | Read-only agent tools backed by stored family evidence |
| Telemetry | Consent boundaries are specified | Anonymous telemetry separate from research traces, disabled by default |

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
repogrammar index
repogrammar sync
repogrammar status
repogrammar doctor
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
src/workers: future language-native semantic workers
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

The next implementation phase is parser, semantic-worker, and IR design:

- implement TypeScript and JavaScript code-unit extraction;
- define the TypeScript semantic worker protocol tests and version policy;
- convert parser AST output into a RepoGrammar-owned unified IR;
- add deterministic fixture coverage under `src/fixtures/`;
- implement repository-local initialization, indexing, and sync only after their
  storage and locking contracts are tested.

See [docs/roadmap.md](docs/roadmap.md) for the staged plan.

## Development

Requirements:

- Rust stable toolchain from [rust-toolchain.toml](rust-toolchain.toml)
- Cargo with rustfmt and clippy components
- Git for diff-based repository guard checks

Required verification:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
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
