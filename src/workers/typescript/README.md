# TypeScript Semantic Worker

This directory contains the bootstrap TypeScript semantic worker entry point.
The checked-in worker is intentionally dependency-free: it validates the v1
stdin request shape and emits typed NDJSON `worker_error` plus `end_of_stream`
messages when compiler-backed semantic analysis is unavailable.

The Rust-side process adapter can send request JSON over stdin, validate NDJSON
v1 stdout, and map sanitized worker failures. This directory still does not
include TypeScript compiler API integration because `tsc`, the TypeScript
compiler API dependency, and package-manager lockfiles are not yet validated in
this repository.

The worker must use a versioned protocol, translate TypeScript compiler or
language-service facts into RepoGrammar-owned semantic facts, and mark
unavailable or incompatible facts as `UNKNOWN`.
