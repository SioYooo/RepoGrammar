# Initialization Progress Specification

Repository initialization and indexing must publish typed progress events.

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

- show an indeterminate spinner until the workload denominator is known;
- show exact completed and total work units plus an exact integer percentage
  when known;
- never display fabricated percentages for unknown work or unstable ETAs;
- support interactive TTY, plain logs, and NDJSON;
- support `--progress auto|always|never`, `--json`, `--quiet`, and `--verbose`;
- remain testable independently of terminal rendering.

## Index generation safety

Initialization and indexing must preserve the previous valid index on
cancellation or failure. They must build a new index generation and atomically
activate it only after validation. Generations live under the repository-local
state directory described in `docs/specifications/storage.md`.

## Current implementation status

The bootstrap defines typed progress stages, known and unknown work units,
plain rendering, and NDJSON rendering. `init` emits repository-state
initialization progress. `index`, `sync`, and `resync` emit typed per-stage
progress events while they run discovery, file metadata storage, syntax
parsing, code-unit normalization, local support-fact recording, semantic-worker
deferred/running status, candidate/family construction, and persistence
validation. Human progress is rendered to stderr with an ASCII bar, integer
percentage, and exact counts when exact work counts are known, and `[working]`
without a percentage when a denominator is not known. `--json --progress
always` emits progress NDJSON on stderr while keeping the final JSON result on
stdout.

Deferred work: semantic workers do not yet provide fine-grained internal
progress through the product CLI, and future mining/provider phases will need
more detailed candidate, provider, and evidence-selection progress events.
