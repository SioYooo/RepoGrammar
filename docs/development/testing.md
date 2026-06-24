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
- Repo-local lifecycle tests must use temporary workspaces and cover init
  layout, idempotent repair, Git exclude hygiene, optional root `.gitignore`
  marker writes, `REPOGRAMMAR_DIR` override validation, symlink/file conflicts,
  human and JSON status/doctor output, corrupted manifests, missing subdirs,
  `uninit --yes`, conservative unlock behavior, and redacted logs metadata.
- File discovery tests must use temporary workspaces and cover TS/JS inclusion,
  unsupported module extensions, default dependency/build/generated/state-dir
  exclusions, Git-ignored files when Git is available, safe Git-unavailable
  warnings, the inclusive 1 MB size boundary, oversized skips, strict SHA-256
  hash generation, deterministic ordering, symlink escape skips, invalid roots,
  and absence of source snippets or absolute paths in reports.
- SQLite storage tests must use temporary workspaces and cover idempotent
  migrations, required-table validation, WAL and foreign-key PRAGMAs,
  foreign-key enforcement, activation pointer validation, preservation of the
  previous active generation after failed validation, repository-relative
  indexed-file paths, and rejection of symlinked or malformed
  active-generation pointers.
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
CLI command surface, missing-index fallback human/JSON output, repo-local
lifecycle init/status/doctor/uninit/unlock/logs safety behavior, TS/JS file
discovery filtering/hash/path-safety behavior, SQLite storage migration and
generation-activation safety behavior, installer dry-run parsing, and
`repo-guard` sync/path/diff/ADR-0008 required document logic.
