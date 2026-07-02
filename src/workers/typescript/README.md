# TypeScript Semantic Worker

This directory contains the TypeScript semantic worker entry point. The
checked-in worker accepts bounded v1 operation requests for module, export,
re-export, and package-entry resolution. It can use a TypeScript compiler API
available from the worker environment or target repository for provider-resolved
module facts, exact export-identity facts, and bounded repo-local
specifier-plus-export binding facts, and otherwise falls back to
dependency-free structural `UNKNOWN`/diagnostic facts.
Application promotion is still role-scoped: imported Drizzle query anchors, for
example, require every requested repo-local `db` and table binding proof before
support is derived.

The Rust-side process adapter can send request JSON over stdin, validate NDJSON
v1 stdout, and map sanitized worker failures. This directory still does not
bundle a TypeScript compiler dependency or package-manager lockfile; broad
`Program`/`TypeChecker` construction remains future work.

The worker must use a versioned protocol, translate TypeScript compiler or
language-service facts into RepoGrammar-owned semantic facts, and mark
unavailable or incompatible facts as `UNKNOWN`.
