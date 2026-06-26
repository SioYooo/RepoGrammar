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
- honor `REPOGRAMMAR_TELEMETRY=0`, `DO_NOT_TRACK=1`, and CI disablement;
- open no telemetry network connection when disabled;
- support status, on, off, purge, local export, and explicit upload commands;
- remain disabled in CI by default.

The implemented anonymous telemetry payload schema is
`src/protocol/telemetry-v1.schema.json` and is described in
`docs/specifications/telemetry-schema-v1.md`. Payloads are validated against the
allowlist before they are written to a repo-local upload queue and before an
explicit upload attempt. The implementation validates required fields, bucket
enums, anonymous identifier shape, and the non-HTTPS localhost exception rather
than accepting lookalike hosts such as `localhost.example`.

## Local aggregation and global preference

Telemetry preference and the anonymous machine id may live in global user state.
Repository-derived telemetry rollups and unsent queues live under
`.repogrammar/telemetry/` for the current repository.

Repo-local telemetry files must not contain source snippets, raw prompts,
absolute paths, symbol names, query text, repository names, repository root
hashes, content hashes, byte ranges, raw targets, credentials, raw environment
variables, or raw error messages. They may use typed event names, coarse
buckets, schema versions, anonymous machine id, external-dependency risk
buckets, typed error codes, and bucketed/category experiment aggregates for
local paired token measurements.

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

The current CLI exposes this separate consent under `repogrammar telemetry
research-status`, `research-on`, `research-off`, `research-export`, and
`research-purge`. Research export is redacted metadata only. Full prompt/source
trace export remains deferred and must require a separate explicit confirmation
if it is ever added.

## Current implementation status

The v0.1 implementation stores anonymous telemetry preference and a random
anonymous machine id in global user state. `repogrammar telemetry status`,
`on`, `off`, `export`, `upload`, and `purge` are implemented. Telemetry export
is inspect-only and does not create a queue or rollup. Telemetry is off by
default, `--yes` during install does not imply telemetry consent, `--telemetry`
during live install persists anonymous telemetry only after agent installation
succeeds, and live `install --yes` without `--telemetry` or `--no-telemetry`
does not prompt and keeps telemetry disabled. Interactive telemetry prompts are
allowed only for a future live install mode that runs without `--yes` and
without explicit telemetry flags. `REPOGRAMMAR_TELEMETRY=0`, `DO_NOT_TRACK=1`,
or CI forces effective telemetry off and skips consent prompting.
Telemetry status reports effective environment disablement, CI disablement,
rollup/queue/sent counts, endpoint configuration, and whether an explicit
upload would open a network connection.
`repogrammar stats --json` remains local and never uploads; when anonymous
telemetry is effectively enabled it may update one allowlisted bucketed rollup
under `.repogrammar/telemetry/rollups/` without creating an upload queue or
opening a network connection.
Local experiment recording remains separate from anonymous telemetry consent.
`experiment-start --yes` is the non-interactive confirmation path; interactive
product runs without `--yes` prompt with default-no `[y/N]`, and the
controlled-pair prompt warns about additional token usage, time, and provider
cost.
When telemetry is disabled, upload returns without opening a network
connection. Upload is explicit only; no MCP request path performs telemetry
network I/O.
Telemetry upload must not add a heavy HTTP client, async runtime, background
worker, or production ingestion dependency solely for v0.1 telemetry. If real
HTTPS upload would require substantial dependency changes, keep the upload
behind `TelemetryUploadTransport` with fake/mock transport tests and preserve
`upload --dry-run` plus parseable no-endpoint fallback behavior.

Repo-local telemetry state lives under `.repogrammar/telemetry/` and may hold
coarse aggregate rollups, queue files, and upload receipts. It must not contain
source snippets, prompts, query text, paths, repository names, symbols,
content hashes, byte ranges, credentials, environment variables, evidence text,
or raw error messages. Source-span telemetry, if added, is limited to
aggregate/bucketed counts such as whether a source-span opt-in was used and how
many spans were returned or omitted.
