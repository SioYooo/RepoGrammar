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

## Research trace collection

Research trace collection is not anonymous product telemetry. It requires a
separate explicit consent path and must not be enabled by product telemetry
settings.

## Current implementation status

The bootstrap defines consent types, environment-disable handling, and the
anonymous allowlist schema. No telemetry network transport exists.
