# Telemetry Specification

Anonymous product telemetry and research trace collection are separate consent
decisions.

## Anonymous telemetry requirements

Anonymous telemetry must:

- use a documented, versioned allowlist schema;
- aggregate usage locally before sending;
- never contain code, paths, repository names, symbols, prompts, query text,
  evidence text, environment variables, credentials, or raw error messages;
- use coarse buckets and typed error codes;
- impose no latency on MCP calls;
- honor `REPOGRAMMAR_TELEMETRY=0` and `DO_NOT_TRACK=1`;
- open no telemetry network connection when disabled;
- support status, on, off, purge, and local export commands;
- remain disabled in CI by default.

## Local aggregation and global preference

Telemetry preference and the anonymous machine id may live in global user state.
Repository-derived telemetry rollups and unsent queues live under
`.repogrammar/telemetry/` for the current repository.

Repo-local telemetry files must not contain source snippets, raw prompts,
absolute paths, symbol names, query text, repository names, credentials, raw
environment variables, or raw error messages. They may use typed event names,
coarse buckets, schema versions, anonymous machine id, hashed repository root,
and typed error codes.

Global state must not contain repository-specific SQLite indexes or
source-derived family/evidence facts.

## Logs are not telemetry

Local diagnostic logs live under `.repogrammar/logs/` and are controlled
separately from telemetry consent. Turning telemetry off must not disable local
diagnostic logs.

Logs must be redacted by default. Repo-local logs may include repo-relative
paths for diagnosis, but telemetry must not upload paths. `debug` and `trace`
logging must not be enabled by default.

## Research trace collection

Research trace collection is not anonymous product telemetry. It requires a
separate explicit consent path and must not be enabled by product telemetry
settings.

## Current implementation status

The bootstrap defines consent types, environment-disable handling, and the
anonymous allowlist schema. No telemetry network transport exists.
