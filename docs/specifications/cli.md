# CLI Specification

RepoGrammar's CLI is designed around implementation-pattern families, not
generic symbol graph navigation.

## v0.1 command surface

Project lifecycle:

- `init`
- `uninit`
- `index`
- `sync`
- `status`
- `doctor`
- `unlock`
- `logs`

Pattern-family queries:

- `find`
- `families`
- `family`
- `member`
- `explain`
- `check`
- `files`
- `units`

Agent integration:

- `serve`
- `install`
- `uninstall`

Metrics:

- `stats`
- `telemetry`

Maintenance:

- `version`
- `help`

## Pattern-family commands

`repogrammar find` is the main human-facing equivalent of the MCP
`repogrammar_context` operation `find_analogues`. It must return candidate
families, target compatibility, dominant patterns, variation points, exceptions,
unknowns, and a minimal contrastive evidence set. It must not return only top-k
similar files.

`repogrammar family` is the CLI equivalent of the `show_family` operation.

`repogrammar explain` is the CLI equivalent of the `explain_deviation`
operation.

`repogrammar check` is the CLI equivalent of the `check_conformance` operation.
Because this slice does not prove runtime equivalence, a matched `check`
response must use `CONTEXT_ONLY` for machine-readable context success and keep
the conformance result advisory `UNKNOWN`.

All query commands must support:

- `--project <path>`
- `--token-budget <n>`
- `--mode compact|evidence|deep`
- `--json`
- `--include-variations`
- `--include-exceptions`

## Long-running commands

All long-running commands must support:

- `--progress auto|always|never`
- `--json`
- `--quiet`
- `--verbose`

Long-running commands include repository initialization, indexing, sync, and MCP
serving.

## Repository state commands

`repogrammar init` creates repository-local state under `.repogrammar/` by
default, or under `REPOGRAMMAR_DIR` when that environment variable is set. It
must not modify tracked repository files by default. It must write
`.repogrammar/` and `.repogrammar-*/` to `.git/info/exclude` when Git is
available, and it must create `.repogrammar/.gitignore` as a second defense.
`REPOGRAMMAR_DIR` is a repo-local directory-name override only; empty values,
absolute paths, traversal, nested paths, symlink state directories, file
conflicts, and names outside `.repogrammar` or `.repogrammar-*` must be
rejected.

`repogrammar init --write-gitignore` may update the root `.gitignore` with a
small marker-fenced section. Without this flag or explicit interactive
confirmation, root `.gitignore` must remain untouched.

`repogrammar uninit` removes repository-local RepoGrammar state. It is the only
command that may remove `.repogrammar/`; `repogrammar uninstall` must not remove
project indexes. `uninit` must make logs deletion explicit.

`repogrammar status` must support human and `--json` output. It must report
whether the repository is initialized, manifest status, the active generation,
manifest schema version, storage schema version, journal mode,
storage/indexing implementation status, missing subdirectories, and relevant
warning states. Status JSON must use `manifest_schema_version` and
`storage_schema_version`; it must not use an ambiguous top-level
`schema_version` field. When storage is wired, it must also report SQLite
integrity status and unhealthy storage states without exposing absolute paths.
Manifest status must be based on parsed JSON fields, not literal text layout,
so valid reordered manifests are accepted and malformed required fields are
reported as corrupted.

`repogrammar doctor` must support human and `--json` output. It must check
manifest status, required lifecycle subdirectories, storage/indexing
implementation status, lock state, Git hygiene, and state directory
configuration. Once SQLite exists, it must also check database integrity,
schema version, journal mode, and active generation consistency.
Doctor must validate generated lifecycle hygiene without mutating state:
`.repogrammar/.gitignore`, `receipts/init.json`, `.git/info/exclude`, and root
`.gitignore` RepoGrammar marker sections must be reported as missing or invalid
rather than silently repaired. JSON output must expose this as
`checks.lifecycle_hygiene`. JSON output must expose index-lock diagnostics as
`checks.locks` with `pass`, `warning`, `fail`, or `not_applicable`.
Doctor JSON must use `checks.manifest_schema_version` and
`checks.storage_schema_version`; it must not expose an ambiguous
`checks.schema_version` field.
During the current syntax-only phase, `doctor` is wired to SQLite storage health
for the active generation. It must still distinguish file-manifest-only,
syntax-only code-unit, and future family-evidence indexing.

`repogrammar index` and `repogrammar sync` currently require an initialized
repository-local state directory. The implemented bootstrap path runs TS/JS and
Python `.py` discovery, reads source through a
repo-relative hash-checked boundary, store repo-relative file metadata and
syntax-only code-unit records plus any syntax-origin framework-role fact records
in a new generation-scoped SQLite database, validate the generation, and
atomically activate
`.repogrammar/current-generation`. Human and JSON output must report
`indexing: syntax_only_code_units`, the actual `indexed_units` count,
the actual `semantic_facts` count, `parser: syntax_only`, `semantic_worker`,
and `mining: deferred`. By default, `semantic_worker` is `deferred`.
During the current TS/JS and Python framework-role slices, `semantic_facts` may
be greater than zero even when `semantic_worker` is `deferred`; those records
are syntax-origin `FRAMEWORK_ROLE` facts with `FRAMEWORK_HEURISTIC` certainty,
Python parser-origin structural/`UNKNOWN` facts, or root `pyproject.toml`
`PROJECT_CONFIG`/config-`UNKNOWN` records. Python exact-anchor derivation may
also add separate `DATAFLOW_DERIVED` support facts without running a semantic
worker. These are bounded RepoGrammar support facts, not compiler/provider-backed
facts.
When `REPOGRAMMAR_TYPESCRIPT_WORKER` is set to an explicit worker executable,
`index` and `sync` may run that worker after syntax-only code units are stored
for the building generation.
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` may supply an optional JSON array of
non-blank string arguments. This is an argv contract, not shell parsing; worker
arguments without `REPOGRAMMAR_TYPESCRIPT_WORKER` are invalid. Worker facts must
pass the same-generation storage gate before they are recorded. Worker
unavailable, unsupported-version, timeout, crash, or protocol-violation
failures must fall back to syntax-only indexing with a typed
`semantic_worker: fallback_*` status and sanitized warnings. A worker fact that
conflicts with the indexed code-unit path, content hash, or range must abort the
new generation rather than silently dropping or accepting stale evidence. If
storage health is already unhealthy, index and sync must refuse and direct the
user to `repogrammar doctor` rather than masking the corruption with a new
generation. Before discovery, source reads, generation preparation, validation,
and activation, both commands acquire `.repogrammar/locks/index.lock` and hold
it through validation and activation.
The lock records process id, host when available, OS, start time, and
RepoGrammar version. Active or unknown lock ownership is refused with guidance
to run `repogrammar doctor`; confirmed stale same-host locks may be replaced
during acquisition. Successful runs remove only the lock content they wrote.

`repogrammar unlock` must remove only confirmed stale locks. It must inspect the
recorded process, host, OS, and advisory lock state before deletion. `--force`
must require explicit confirmation. Without `--force --yes`, unlock is
inspection-only. With `--force --yes`, it may remove only a confirmed stale
`index.lock`; active, unknown, invalid, daemon, and SQLite locks must remain in
place with a stable refusal reason.

`repogrammar logs` reads repo-local diagnostic logs. It supports:

- `--tail`;
- `--since <duration>`;
- `--component index|daemon|mcp|telemetry`;
- `--redact`.

Logs are diagnostic state, not telemetry. During bootstrap, `logs` exposes
redacted metadata only; machine-readable output must not include source
snippets or absolute paths.

## Installer commands

`install` and `uninstall` must support:

- `--target`
- `--scope global|project`
- `--dry-run`
- `--yes`
- `--print-config`
- `--no-telemetry`
- `--no-permissions`

Installer commands configure agents and machine-level integration only. They do
not create, delete, or rewrite `.repogrammar/`. Live writes require `--yes`.
The current implementation supports explicit `--target codex --scope global`
through the native Codex MCP CLI and explicit
`--target claude-code --scope global` through the native Claude Code MCP CLI.
Live `--target all` and all project-local writes remain deferred to avoid
partial or unsupported agent configuration. `install` runs a read-only MCP
self-test before native configuration and writes a managed receipt after native
configuration succeeds; `uninstall` removes only receipt-owned managed entries.

## Metrics commands

`repogrammar stats` reports metric availability and measurement taxonomy. During
bootstrap it does not read repository metrics, token counts, or family evidence.
With `--json`, it must return a parseable deferred object with
`implemented: false`, an empty `metrics` array, the allowed metric kinds
`MEASURED`, `DERIVED`, `ESTIMATED`, and `CAUSAL_EXPERIMENT`, and `null` values
for token savings and context compression ratio. It must not include paths,
source snippets, query text, repository names, or any inferred token-savings
claim.

## Disallowed top-level graph commands

The following CodeGraph-style names must not be added as top-level v0.1
commands:

- `callers`
- `callees`
- `impact`
- `affected`
- `node`
- `explore`

If call-graph functionality is later needed, it must live under a secondary
namespace such as `repogrammar graph callers` and must not be presented as the
primary value proposition.

An optional lower-layer provider such as CodeGraph must not change this command
surface. Provider facts may enrich future pattern-family evidence only after
translation into RepoGrammar-owned evidence with provenance and freshness.

## Missing or stale indexes

Query commands must check project initialization and freshness before making
family claims. If `.repogrammar/` is missing, the command must return clean
fallback guidance rather than panic or implicitly initialize the repository:

```text
FALLBACK_TO_CODE_SEARCH
reason: repository is not initialized
guidance: run repogrammar init
```

During the bootstrap, pattern-family query commands use this fallback shape only
when repository state or an active index generation is missing or unreadable.
`status` and `doctor` may report a clean not-initialized state without opening
storage. Stored syntax-only code units are not family evidence; stored
syntax-origin framework-role facts remain insufficient support unless stronger
compatible semantic/dataflow evidence exists. Exact-anchor Python
`DATAFLOW_DERIVED` support may produce narrow EC-MVFI-lite family rows when the
family support threshold is met, but query commands must not imply that
provider-backed Python v0.1 analysis, TypeScript compiler analysis, full
mining, or broad production family evidence has run. The
`files` and `units` commands are a limited exception: when an active
file-manifest-only or syntax-only generation exists, they may read and return
repo-relative indexed-file metadata and code-unit records for inventory/debugging
only.
The query application layer now owns a shared preflight contract so pattern
family commands enter the active FamilyStore read path once a readable active
generation exists, while `files` and `units` are implemented inventory commands
whose fallback means an active inventory index precondition is missing or
unreadable. Semantic-fact freshness/readiness checks remain internal and must
not introduce semantic-fact CLI output. The presence of FamilyStore tables is
not by itself a strong claim: the query layer must return typed `UNKNOWN` when
support, compatibility, or evidence is insufficient. MCP serving uses the same
query preflight and family lookup boundary rather than a separate claim path.

With `--json`, query fallback output must use exit status `2` and write a
stable JSON object to `stderr` rather than the human text block:

```json
{
  "status": "FALLBACK_TO_CODE_SEARCH",
  "reason": "repository is not initialized",
  "guidance": "run repogrammar init",
  "command": "find",
  "implemented": false
}
```

For `files` and `units`, the command itself is implemented even when its active
index precondition is not met. Their missing/unreadable-index JSON fallback must
therefore set `implemented: true`; pattern-family query commands set
`implemented: false` only for missing/unreadable active index fallback and set
`implemented: true` when they return typed `UNKNOWN` from a readable active
generation.

If the index is stale, the command must warn or refuse claims whose evidence has
changed.

Analysis uncertainty must be reported as typed `UNKNOWN` with a reason code and
affected claim. Missing-index fallback, deferred stronger query evidence,
stale-index refusal, and typed `UNKNOWN` are separate states and must not be
collapsed into one error string.

If repository-local state exists but no active generation exists, query commands
must keep the same `FALLBACK_TO_CODE_SEARCH` marker with `reason: no active
index generation`, not `repository is not initialized`. If a readable active
generation exists but no supported family evidence has been built, query
commands must return typed `UNKNOWN` with `InsufficientSupport` instead of the
fallback marker.
If repository status cannot be read safely, the fallback must direct the user to
`repogrammar doctor` instead of masking corrupted state as an uninitialized
repository.
For `files` and `units`, initialized state with no active generation must keep
the fallback marker but use `reason: no active index generation`, guidance to
run `repogrammar index`, and `implemented: true` in JSON. Corrupt or unreadable
state must direct users to `repogrammar doctor`. Once an active
file-manifest-only or syntax-only generation exists, `files --json` must return
`status: ok`, `implemented: true`, the active generation, an `indexing` value of
either `file_manifest_only` or `syntax_only_code_units`, and a `files` array of
repo-relative paths, languages, sizes, and strict content hashes. `units --json`
must return the active generation, the same `indexing` contract,
`semantic_worker: deferred`, `mining: deferred`, and a `units` array of
repo-relative unit ids, paths, languages, kinds, byte ranges, and strict content
hashes. Neither command may include source snippets or absolute paths.

For active pattern-family commands, `families --json` returns `status: ok` and a
`families` array when family rows exist; otherwise it returns `status: UNKNOWN`,
`implemented: true`, and a typed `InsufficientSupport` unknown on stdout.
`family`, `member`, `find`, `explain`, and `check` accept the first positional
operand as their target. `family <target>` is an exact family-id lookup.
`member <target>` is an exact code-unit/member-id lookup. `find`, `explain`,
and `check` may use fuzzy matching over supported query-safe path suffixes,
exact member roles, and exact ids, but must not treat short substrings such as a
framework name, classification label, or directory fragment as a successful
family match. Matched family output defaults to `--mode compact`: family id,
classification, support, members, variation slots, typed unknowns, selected
output metadata, and no evidence records or source snippets. `--mode evidence`
adds budgeted repo-relative evidence metadata: evidence id, family id,
code-unit id, path, content hash, byte range, note, estimated token cost, and
covered claim labels. `--token-budget <n>` validates a positive integer and
implies `--mode evidence` unless an explicit mode is provided. Evidence mode
uses deterministic greedy marginal coverage per estimated token cost. Stored
family evidence carries schema-backed `covered_claims` labels from the
allowlist `canonical`, `support`, `variation`, and `exception`; the selector
must consume those labels rather than inferring coverage from note text or
storage order. The current family builder emits `canonical` and `support`
labels, plus a narrow Python `variation` label when an already-ready family has
multiple exact-compatible framework-anchor support targets. `--include-exceptions`
and broader variation requests must still report missing coverage until later
builders explicitly link evidence to variation slots or exceptions.
`--mode deep` is accepted as an
explicit detail request, but until a safe source-span rendering contract exists
it remains metadata-only and must report `source_snippets_included: false`.
None of these modes may include absolute paths or source snippets. `check` is
advisory in this slice: it may return matched family context as
`CONTEXT_ONLY`, but the check-specific conformance status remains `UNKNOWN`
with reason `runtime equivalence remains unproven`.

Before public pattern-family output is returned, stored family evidence must be
fresh against the current repository source hashes. If an evidence source is
missing or its content hash no longer matches the active generation, public
`families`, `family`, `member`, `find`, `explain`, and `check` output must
refuse or omit the stale claim and return typed `StaleEvidence` `UNKNOWN`
guidance instead of rendering stale family detail.

## Current implementation status

The bootstrap recognizes the command surface and required options. `init`
creates safe repo-local lifecycle state, `.repogrammar/.gitignore`, required
lifecycle subdirectories, a bootstrap manifest, `receipts/init.json`, and Git
ignore hygiene. `uninit --yes` removes only the resolved RepoGrammar state
directory. `status`, `doctor`, `unlock`, and `logs` expose human and JSON-safe
repo-local lifecycle information without claiming parser/mining support.
`index` and `sync` currently create syntax-only SQLite generations from the
TS/JS file discovery substrate plus the Python `.py` discovery/CPython AST
structural extractor. Their JSON output
includes `generation_id`, `discovered_files`, `stored_files`, the actual
`indexed_units` count, the actual `semantic_facts` count, `indexing:
syntax_only_code_units`, `parser: syntax_only`, `semantic_worker`, and `mining:
deferred`. The structural extractors can also produce syntax-origin
framework-role fact records for recognized Express, React, Jest/Vitest,
FastAPI, pytest, Pydantic, and SQLAlchemy code-unit shapes; these may increase
`semantic_facts` while `semantic_worker: deferred` remains true. Python
parser-origin structural facts and root `pyproject.toml` project-config records
may also increase `semantic_facts` without changing `semantic_worker:
deferred`. Exact-anchor Python `DATAFLOW_DERIVED` support facts may also be
stored in this default path. By default the
commands do not launch a semantic worker and report
`semantic_worker: deferred`. When
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies the worker argv vector. The
commands pass the discovered repo-relative TS/JS file set to that worker, record
only worker facts that match the active building-generation code-unit
path/hash/range gate.
They do not store source snippets or absolute paths. The product indexing path
now runs a conservative EC-MVFI-lite family builder before activation. That
builder can write family records only when compatible framework-role candidates
also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED` support; the
default syntax-origin framework-role facts and raw parser facts alone still
produce no family rows.
`files` and `units` now read only active file-manifest-only or syntax-only index
metadata and, when present, code-unit records. Pattern-family query commands
return missing-index fallback before an active generation exists, typed
`UNKNOWN` when active family evidence is insufficient, and stored family detail
when EC-MVFI-lite has written supported family rows. Stored family detail now
uses compact/evidence/deep output modes. Compact is the default and omits
evidence records; evidence and deep currently return selected metadata only and
do not include source snippets. `serve` runs the read-only MCP
`repogrammar_context` stdio boundary and reuses the same query preflight and
FamilyStore-backed lookup path. Commands that install or uninstall agent
configuration now support narrow explicit-target live writes after MCP
self-test. The CLI now includes the first Python structural indexing slice, but
Pyrefly/Pyright provider evidence, richer repo-local import semantics, safe
source-span deep output, and broad Python family mining remain deferred. Narrow
exact-anchor Python family rows may exist when EC-MVFI-lite has enough derived
support. Unsupported live target/scope combinations return explicit deferred
errors; dry-run planning remains available
for all targets and scopes.
