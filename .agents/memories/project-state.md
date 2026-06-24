# Project State

- Status: Bootstrap
- Last updated: 2026-06-24
- Scope: Current implemented capability snapshot.
- Evidence: Rust skeleton, docs, CI, and `repo-guard` bootstrap files.
- Related canonical docs: `README.md`, `docs/roadmap.md`
- Supersedes: None
- Superseded by: None

## Context

RepoGrammar is newly initialized as a Rust repository.

## Durable knowledge

Implemented capabilities are limited to module boundaries, minimal domain
types, pattern-family-first CLI command parsing, safe installer dry-run planning,
typed progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, repository guard checks,
documentation, skills, memories, and CI configuration.

Pattern mining, Tree-sitter parsing, TypeScript semantic worker execution,
SQLite persistence, repository index generations, installer writes, telemetry
network transport, and MCP serving are not implemented.

## Implications

Future agents must not claim TypeScript analysis, Python production support,
pattern-family mining, SQLite indexing, or stable MCP API support until those
capabilities are implemented and tested.

## Revalidation conditions

Update this memory after the first real parser, storage, indexing, or MCP server
implementation lands.
