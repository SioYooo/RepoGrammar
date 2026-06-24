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
- show exact completed and total work units when known;
- never display fabricated percentages or unstable ETAs;
- support interactive TTY, plain logs, and NDJSON;
- support `--progress auto|always|never`, `--json`, `--quiet`, and `--verbose`;
- remain testable independently of terminal rendering.

## Index generation safety

Initialization and indexing must preserve the previous valid index on
cancellation or failure. They must build a new index generation and atomically
activate it only after validation.

## Current implementation status

The bootstrap defines typed progress stages, known and unknown work units,
plain rendering, and NDJSON rendering. It does not yet run indexing.
