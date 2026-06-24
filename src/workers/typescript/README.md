# TypeScript Semantic Worker

This directory is reserved for the future TypeScript semantic worker. The
bootstrap does not include executable TypeScript worker source because `tsc` and
package-manager lockfiles are not yet validated in this repository.

The worker must use a versioned protocol, translate TypeScript compiler or
language-service facts into RepoGrammar-owned semantic facts, and mark
unavailable or incompatible facts as `UNKNOWN`.
