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
Because that state carries a local salt and the anonymous machine id, telemetry
state files are written owner-only (mode `0600`) on Unix so other users on a
shared host cannot read them. Repository-derived telemetry rollups and unsent
queues live under `.repogrammar/telemetry/` for the current repository.

Bucketed ratios must keep a regression distinguishable from break-even: a
negative token-savings ratio (treatment used more tokens than baseline) uses a
distinct `negative` bucket rather than collapsing into the `0` bucket.

Repo-local telemetry files must not contain source snippets, raw prompts,
absolute paths, symbol names, query text, repository names, repository root
hashes, content hashes, byte ranges, raw targets, credentials, raw environment
variables, or raw error messages. They may use typed event names, coarse
buckets, schema versions, anonymous machine id, external-dependency risk
buckets, typed error codes, bucketed/category experiment aggregates for local
paired token measurements, and local-only aggregate estimated token counts for
`estimated_potential_token_savings`.

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
Every CLI family query and MCP context call may best-effort update
`.repogrammar/telemetry/local-metrics/family_query_metrics.json` even when
anonymous telemetry is disabled. Schema `family-query-metrics.v2` stores the
query denominator and optional estimated-savings numerator together: one
`total_queries` increment per invocation plus, for found families,
PARTIAL_CONTEXT read plans, and committed/partial alignment certificates, one
`savings_events` increment and its token totals and breakdowns. Both sides are
validated and persisted by one process-serialized atomic file replacement, so
concurrent CLI and MCP processes cannot overwrite each other's increments. The file carries epoch
`atomic-query-accounting.v2`, `epoch_started_unix_seconds`, and
`producer_version`; stats may only form a ratio within this cohort. Existing
`estimated-potential-token-savings.v1` and `family-query-outcomes.v1` files are
historical unpaired artifacts: v2 does not import, rewrite, or combine them.
The v2 file is separate from anonymous telemetry upload payload v1 and stores
only the closed low-cardinality outcome, entrypoint, command/operation,
lookup-mode, typed UNKNOWN, read-plan/source-span count buckets, aggregate token
counts, and outcome/language savings breakdowns. It must not store code, source
snippets, repository names, absolute or repo-relative paths, symbols, raw
targets, query text, prompts, raw MCP/tool input or output, evidence text,
content hashes, byte ranges, ids, raw errors, diffs, or patches.
Local experiment recording remains separate from anonymous telemetry consent.
The telemetry help surface scopes `--project <path>` to anonymous telemetry
and research diagnostics. Local `experiment-*` subcommands use machine-local
state, accept only their dedicated options, and reject `--project`
rather than suggesting a repository-specific experiment store.
`experiment-start --yes` is the non-interactive confirmation path; interactive
product runs without `--yes` prompt with default-no `[y/N]`, and the
controlled-pair prompt warns about additional token usage, time, and provider
cost. `experiment-record --usage-json <path>` may import counts from a
redacted local usage file instead of requiring manual token flags. The import
path is not stored, accepted files are bounded JSON objects containing only
token counts plus optional success/test-outcome metadata, and unsupported fields
are rejected to prevent raw prompts, messages, source snippets, paths, symbols,
patches, query text, credentials, or errors from becoming telemetry or
experiment state.
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
