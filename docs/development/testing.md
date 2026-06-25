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
  human and JSON status/doctor output, JSON-parsed manifest validation with
  reordered valid fields and invalid required fields, corrupted manifests,
  missing subdirs, diagnostic-only doctor findings for missing or invalid
  `.repogrammar/.gitignore`, `.git/info/exclude`, root `.gitignore` markers,
  and `receipts/init.json`,
  `uninit --yes`, conservative unlock behavior, and redacted logs metadata.
- File discovery tests must use temporary workspaces and cover TS/JS inclusion,
  unsupported module extensions, default dependency/build/generated/state-dir
  exclusions, Git-ignored files when Git is available, safe Git-unavailable
  warnings, parent Git worktree ignore rules for subdirectory projects, the
  inclusive 1 MB size boundary, oversized skips, strict SHA-256 hash
  generation, bounded max-plus-one content reads for hashing,
  deterministic ordering, symlink escape skips, invalid roots, and absence of
  source snippets or absolute paths in reports.
- SQLite storage tests must use temporary workspaces and cover idempotent
  migrations, required-table validation, WAL and foreign-key PRAGMAs,
  foreign-key enforcement, activation pointer validation, preservation of the
  previous active generation after failed validation, repository-relative
  indexed-file paths, semantic-fact/evidence storage with same-generation
  code-unit path/hash/range validation, IR node/edge storage with
  same-generation code-unit/node validation, malformed semantic evidence and IR
  graph rejection before activation, atomic rollback of failed fact writes,
  building-generation write gates for indexed files, code units, IR nodes/edges,
  and semantic facts, validation/activation transition guards that do not
  downgrade active generations, read-only active `files`/`units` listing order
  and tamper rejection, read-only active IR and semantic-fact listing with
  validation and tamper rejection, internal active claim-input snapshot reads
  from one validated generation, snapshot tamper rejection across files, units,
  IR, and semantic facts, and rejection of symlinked or malformed
  active-generation pointers.
- Syntax-only `index` and `sync` tests must cover initialized-state
  requirements, human and JSON output, generation activation, positive code-unit
  extraction and storage, source ranges, language/kind/content-hash metadata,
  malformed syntax returning partial units plus diagnostics, unsupported or
  invalid source behavior, generation preservation after source/parser/storage
  failure, status/doctor storage health, corrupt manifests, missing state
  subdirectories without implicit repair, active `files`/`units` human and JSON
  output, no-active-generation fallback, broken active-generation pointers,
  product runtime wiring, and absence of source snippets or absolute paths in CLI
  output and stored metadata.
- Optional semantic-worker indexing tests must cover explicit opt-in wiring,
  non-empty discovered-file request scope, deterministic fact recording through
  the same-generation storage gate, syntax-only fallback for unavailable,
  unsupported-version, timeout, crash, and protocol-violation worker results,
  sanitized fallback warnings, and preservation of the previous active
  generation when accepted worker output conflicts with indexed
  path/hash/range evidence.
- Protocol fixture tests must parse fixture lines as JSON before checking
  message types, fallback payloads, repository-relative evidence paths,
  sanitized target/note text, evidence fields, and strict content-hash formats.
  Semantic fact target tests must cover invalid blank targets, accepted `null`
  targets, and accepted non-blank targets.
- Semantic-worker request fixture tests must parse the stdin request as JSON and
  reject wrong protocol versions, missing required fields, non-object payloads,
  non-absolute project roots, duplicate changed files, absolute paths,
  traversal, Windows absolute paths, URI-like paths, and backslash paths.
- Runtime semantic-worker adapter tests must cover valid fact/progress/EOS
  output, malformed JSON, missing EOS, invalid hashes, blank targets,
  impossible work counts, absolute or URI evidence paths, unsupported snippet
  fields, sanitized worker-error mapping, worker crashes, timeouts, oversized
  output, invalid request paths, unrequested fact paths, and relative executable
  rejection. They must also cover inherited-pipe timeout handling, unsupported
  field-name redaction, invalid/symlink project roots, oversized request guards,
  worker-error output that omits `end_of_stream`, unsupported TypeScript
  versions with semantic certainty, sorted/deduplicated request files, and
  rejected absolute-path or source-like free text.
- TypeScript worker executable tests must run the dependency-free worker stub
  through Node, validate parseable NDJSON `worker_error` plus `end_of_stream`
  output for valid requests, reject malformed requests, and prove request paths
  are not echoed in errors.
- Experimental Python dogfooding tests, once added, must be opt-in and must
  assert experimental support level plus typed `UNKNOWN` for dynamic imports,
  monkey patching, pytest fixture injection, runtime dependency injection,
  unresolved imports, and framework magic.
- Optional provider tests, once added, must cover provider absent, present,
  stale, and conflicting states without making CodeGraph or any other provider
  required for default tests.
- UNKNOWN governance tests must cover blocking, non-blocking, recoverable, and
  irreducible unknowns when those classes enter Rust, CLI, MCP, storage, or
  metrics code.
- Stats CLI tests must cover the human deferred message, parseable `--json`
  output, allowed metric-kind vocabulary, null token-savings fields, unknown
  option rejection, and absence of source/path leakage.
- Progress tests must cover invalid known-work counts through the `WorkUnits`
  constructor rather than constructing impossible progress states directly.

## Current coverage

Bootstrap tests cover core model validation, classification vocabulary,
measurement taxonomy, semantic certainty behavior, protocol token mappings,
strict content-hash validation, TypeScript worker version fallback, progress
rendering and `WorkUnits` validation, schema coverage, JSON-parsed semantic
worker request and NDJSON fixture coverage, Rust-side TypeScript semantic-worker
process and NDJSON validation behavior, telemetry consent, transport-neutral MCP
tool names, CLI command surface, missing-index fallback human/JSON output,
repo-local lifecycle init/status/doctor/uninit/unlock/logs safety behavior,
JSON-parsed bootstrap manifest validation,
TS/JS file discovery filtering/hash/path-safety behavior, SQLite storage
migration and generation-activation safety behavior, validated
semantic-fact/evidence storage substrate behavior, syntax-only code-unit
extraction and storage bridging, source-read hash/path safety, storage-aware
status/doctor reporting, active file-manifest-only or syntax-only
`files`/`units` read paths, product runtime wiring, optional semantic-worker
fact ingestion through the
same-generation storage gate, sanitized worker fallback during indexing,
structural IR node/containment-edge storage for syntax-only code units,
active semantic-fact/evidence read-path validation plus internal active
claim-input snapshot validation for future claim builders, typed UNKNOWN
class/reason token validation, internal semantic-fact freshness/readiness gating
for fresh supported facts, stale evidence, missing source, weak certainty,
conflicting facts, and `UNKNOWN` fact kind,
dependency-free TypeScript worker unavailable-stub behavior,
installer dry-run parsing, deferred `stats --json` metrics contract behavior,
bounded filesystem source reads for discovery hashing and source-store
hash-checked reads, parent Git worktree ignore handling for subdirectory
projects, and `repo-guard` sync/path/diff/ADR-0008 required document logic.
