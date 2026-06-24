# ADR-0004: Rust core with language-native semantic workers

- Status: Accepted
- Date: 2026-06-24

## Context

Tree-sitter is suitable as a universal, tolerant syntax layer, but it cannot
fully resolve symbols, module targets, overloads, aliases, generic types,
decorator semantics, or dynamic framework bindings. RepoGrammar needs high
precision semantic evidence without making the core depend directly on every
language compiler runtime.

## Decision

Use Rust as the primary implementation language for the core analysis engine,
indexing engine, storage, CLI, MCP server, and repository governance tools. Use
Tree-sitter as the universal syntax frontend and fallback parser.

Allow language adapters to use the native compiler, type-checker, or LSP
ecosystem that provides authoritative semantic information. The TypeScript and
JavaScript adapter may use a TypeScript semantic worker behind a versioned
process protocol.

All language-specific AST, compiler, SDK, and LSP types must be translated into
RepoGrammar-owned semantic facts and unified IR before entering the Rust core.

## Alternatives considered

- Pure Rust: strong core implementation, but insufficient for complete
  TypeScript semantics without reimplementing official compiler behavior.
- Pure TypeScript: faster TS/JS prototype and MCP iteration, but weaker fit for
  a long-term local multi-language static-analysis engine.
- Tree-sitter only: good syntax baseline, but not enough for symbol, type,
  overload, alias, module, and framework semantics.

## Consequences

The repository layout reserves `src/rust/` for the Rust core and
`src/workers/` for future native semantic workers. Structural similarity can
generate candidates but cannot alone prove semantic family membership.
Compiler-native facts take precedence over structural heuristics. Conflicting or
unavailable facts become `UNKNOWN` or abstention.

## Follow-up work

Design and validate the TypeScript worker toolchain, version policy, protocol
tests, and package lockfile before adding executable worker code.
