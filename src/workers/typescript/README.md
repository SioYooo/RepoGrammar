# TypeScript Semantic Worker

This directory is reserved for the future TypeScript semantic worker. The
Rust-side process adapter can already send request JSON over stdin, validate
NDJSON v1 stdout, and map sanitized worker failures. This directory still does
not include executable TypeScript compiler worker source because `tsc`, the
TypeScript compiler API dependency, and package-manager lockfiles are not yet
validated in this repository.

The worker must use a versioned protocol, translate TypeScript compiler or
language-service facts into RepoGrammar-owned semantic facts, and mark
unavailable or incompatible facts as `UNKNOWN`.
