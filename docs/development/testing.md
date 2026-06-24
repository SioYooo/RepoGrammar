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

## Current coverage

Bootstrap tests cover core model validation, classification vocabulary,
semantic certainty behavior, transport-neutral MCP tool names, CLI version and
not-implemented behavior, and `repo-guard` sync/path/diff logic.
