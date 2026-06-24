# ADR-0001: Rust core and local binary

- Status: Accepted
- Date: 2026-06-24

## Context

RepoGrammar is intended to run locally as a CLI and MCP server with static
analysis, local indexing, and conservative evidence handling. It needs a
distributable binary and clear adapter boundaries for parser and storage
dependencies.

## Decision

Use Rust for the core engine and local binaries. Keep the Rust core as a single
Rust package during bootstrap, with room for future workspace evolution when
module boundaries and release needs justify it. This does not require every
language adapter to be implemented in Rust; see ADR-0004.

## Alternatives considered

- TypeScript and Node backend: faster iteration for JSON and MCP integration,
  but less attractive for a single local analysis binary and long-running
  repository scans.
- Multi-crate Rust workspace immediately: clearer packaging boundaries, but
  premature during bootstrap.

## Consequences

Rust becomes the default implementation language for core logic, CLI, MCP server
skeleton, repository guard, and local indexing. Language-native semantic workers
may use their native ecosystem behind a versioned protocol.

## Follow-up work

Evaluate actual parsing, storage, and MCP dependencies before adding production
crates. Do not claim performance benefits until measured.
