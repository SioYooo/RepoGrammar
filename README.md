# RepoGrammar

RepoGrammar is a pre-alpha local analysis engine for discovering repository
implementation pattern families and returning auditable source evidence to
coding agents.

## Current status

This repository is in repository bootstrap state. It contains governance,
documentation, CI, a Rust core skeleton, semantic-worker boundaries, a small CLI boundary, and repository
guard checks. It does not yet implement pattern mining, indexing, storage
migrations, or a working MCP server.

## Design goals

- Analyze repositories locally without cloud services.
- Parse source with Tree-sitter adapters as a universal syntax layer and convert
  parser output into a RepoGrammar-owned unified IR.
- Accept language-native semantic facts through versioned workers, starting with
  a future TypeScript worker that can use official compiler or language-service
  APIs.
- Mine pattern families with canonical templates, variation points,
  exceptions, companion families, and provenance.
- Store local index metadata and source evidence in SQLite with FTS5.
- Return `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, or `UNKNOWN` rather than
  overclaiming from weak evidence.
- Optimize v0.1 for production-quality TypeScript/JavaScript pattern-family
  evidence. Python is planned as the second official language and is not part of
  the v0.1 production scope.

## Non-goals

- No local LLM, embedding model, vector database, or cloud API in the first
  version.
- No automatic modification of user business code from pattern-family results.
- No production Python support in v0.1. Any earlier Python adapter must be
  experimental and clearly labeled.
- No claim of production readiness, stable MCP API, token savings, or completed
  pattern mining.

## Architecture overview

RepoGrammar uses a Rust primary core with room for language-native semantic
workers under `src/`:

```text
src/rust: Rust core, analysis engine, CLI, MCP, storage, repository guard
src/workers: future language-native semantic workers
src/protocol: versioned worker protocol documents and schemas
```

Tree-sitter is the universal syntax frontend and fallback parser. It generates
structural candidates, not final semantic truth.

See `docs/architecture/overview.md` and `docs/architecture/module-map.md`.

## Current commands

```text
cargo run --quiet --bin repogrammar -- version
cargo run --quiet --bin repogrammar -- help
cargo run --quiet --bin repogrammar -- status
cargo run --quiet --bin repogrammar -- doctor
cargo run --quiet --bin repogrammar -- find --project . --token-budget 8000 <target>
cargo run --quiet --bin repogrammar -- install --dry-run --target codex --scope project
cargo run --quiet --bin repo-guard -- check
```

The v0.1 command surface is pattern-family-first. `find`, `family`, `explain`,
and `check` are the CLI equivalents of the first MCP pattern-family tools.
Repository mutation, indexing, installer writes, and MCP serving still return
explicit not-implemented errors until the corresponding storage and integration
contracts are implemented.

The following graph-navigation names are intentionally not top-level v0.1
commands: `callers`, `callees`, `impact`, `affected`, `node`, and `explore`.

## Development requirements

- Rust stable toolchain from `rust-toolchain.toml`
- Cargo with rustfmt and clippy components
- Git for diff-based repository guard checks

## Build and verification

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --quiet --bin repo-guard -- check
```

## Documentation entry points

- `docs/README.md`: documentation map and precedence.
- `AGENTS.md` and `CLAUDE.md`: mirrored mandatory agent contract.
- `docs/development/agent-workflow.md`: contribution workflow.
- `.agents/skills/`: reusable agent procedures.
- `.agents/memories/`: durable project context that is not normative.
