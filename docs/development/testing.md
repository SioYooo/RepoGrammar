# Testing Policy

All test source lives under `src/`.

## Locations

- Module-level tests use `#[cfg(test)] mod tests` beside implementation.
- Crate-level Rust integration-style tests live in `src/rust/integration_tests/`.
- Shared deterministic Rust helpers live in `src/rust/test_support/`.
- Source fixtures live in `src/fixtures/`.

Root `tests/`, `benches/`, `examples/`, and `scripts/` directories are not
allowed.

## Test properties

- Tests must be deterministic and independent of execution order.
- Tests must not access the network by default.
- Temporary directories must be unique and cleaned up.
- Tests must not modify real repository files unless the test is explicitly
  exercising a temporary copy.
- CLI not-implemented behavior must be stable and asserted.
- CLI missing-index fallback tests must cover both human-readable output and
  `--json` output for the query command surface.
- Protocol fixture tests must parse fixture lines as JSON before checking
  message types, fallback payloads, evidence fields, and strict content-hash
  formats. Semantic fact target tests must cover invalid blank targets,
  accepted `null` targets, and accepted non-blank targets.
- Progress tests must cover invalid known-work counts through the `WorkUnits`
  constructor rather than constructing impossible progress states directly.

## Current coverage

Bootstrap tests cover core model validation, classification vocabulary,
measurement taxonomy, semantic certainty behavior, protocol token mappings,
strict content-hash validation, TypeScript worker version fallback, progress
rendering and `WorkUnits` validation, schema coverage, JSON-parsed semantic
worker fixture coverage, telemetry consent, transport-neutral MCP tool names,
CLI command surface, missing-index fallback human/JSON output and deferred
implementation status, installer dry-run parsing, and `repo-guard`
sync/path/diff/ADR-0008 required document logic.
