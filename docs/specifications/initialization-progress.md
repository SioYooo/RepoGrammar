# Initialization Progress Specification

Setup orchestration, repository initialization, and indexing must publish typed
progress events.

## Required stages

- project discovery;
- file scanning;
- syntax parsing;
- semantic resolution;
- code-unit extraction and normalization;
- candidate discovery;
- family construction;
- persistence and validation.

## Progress rendering

Progress must:

- show an indeterminate indicator until the workload denominator is known;
- show exact completed and total work units plus an exact integer percentage
  when known;
- never display fabricated percentages for unknown work or unstable ETAs;
- support interactive TTY progress and noninteractive plain logs;
- keep typed progress events serializable for internal tests and future machine
  transports without making the current CLI print NDJSON progress;
- rewrite a single terminal line for interactive TTY progress while keeping
  plain logs append-only;
- support `--progress auto|always|never`, `--json`, `--quiet`, and `--verbose`;
- remain testable independently of terminal rendering.

## Index generation safety

Initialization and indexing must preserve the previous valid index on
cancellation or failure. They must build a new index generation and atomically
activate it only after validation. Generations live under the repository-local
state directory described in `docs/specifications/storage.md`.

## Current implementation status

The bootstrap defines typed progress stages, known and unknown work units,
plain rendering, and internal NDJSON serialization. `init` uses live indexing
progress and a concise result; interactive bootstrap does not append a delayed
synthetic state-initialization event after that result. State-only and explicit
noninteractive progress retain the initialization event. `index`, `sync`, and `resync` emit typed per-stage
progress events while they run discovery, file metadata storage, syntax
parsing, code-unit normalization, local support-fact recording, semantic-worker
deferred/running status, candidate/family construction, and persistence
validation. Human progress is rendered to stderr. Interactive terminals show a cyan,
width-bounded bar, a short phase name, and exact completed/total work with that
phase's integer percentage. Unknown work remains indeterminate without a
percentage or ETA. `NO_COLOR` disables color while retaining the bar. UTF
locales use Unicode bars; other locales use ASCII. `COLUMNS` controls width
(default 80); narrow frames omit detail before exact counts and never invent
truncated numeric values. Carriage-return plus line erasure updates one line,
followed by a final newline. Non-TTY, `CI`, and `TERM=dumb` use append-only
plain ASCII logs. No animation thread or estimated overall progress is added. `--json --progress always` keeps the final
JSON result on stdout while rendering the same human progress-bar output on
stderr.

`setup` exposes the ordered application plan as sanitized stage results for
agent integration, repository initialization, indexing, default-on auto-sync,
and MCP self-test. Standalone `init` follows the same repository order and
starts auto-sync after indexing unless `--no-autosync` is present. Dry-run
renders those stages without executing them. Human progress remains on stderr
and final human or JSON output remains on stdout; neither surface may expose
receipt paths, repository absolute paths, or raw native-agent errors.

Deferred work: semantic workers do not yet provide fine-grained internal
progress through the product CLI, and future mining/provider phases will need
more detailed candidate, provider, and evidence-selection progress events.
