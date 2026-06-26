# Known Constraints

- Status: Active
- Last updated: 2026-06-25
- Scope: Confirmed repository constraints that are easy to violate.
- Evidence: Mirrored root guide, architecture docs, product spec, and
  `repo-guard` checks.
- Related canonical docs: `AGENTS.md`, `docs/architecture/dependency-rules.md`,
  `docs/specifications/product.md`,
  `docs/decisions/ADR-0011-python-first-v0-1.md`,
  `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`
- Supersedes: None
- Superseded by: None

## Context

RepoGrammar intentionally starts with strict repository hygiene and conservative
analysis rules.

## Durable knowledge

- All source, script, fixture, test, benchmark, migration-tool, and automation
  code belongs under `src/`.
- `AGENTS.md` and `CLAUDE.md` must remain byte-identical root files.
- The first version is local-only and does not use LLMs, embeddings, vector
  databases, or cloud APIs.
- Repository-derived analysis state belongs in repo-local `.repogrammar/` or
  `REPOGRAMMAR_DIR`; global state must not store code-derived family facts,
  evidence, source paths, symbols, prompts, query text, or repo-specific SQLite
  indexes.
- `REPOGRAMMAR_DIR` is a repo-local directory-name override only, not an
  arbitrary global database path.
- `install` and `uninstall` must not create or remove `.repogrammar/`; `init`
  and `uninit` own project state lifecycle.
- `index` and `sync` currently store repo-relative file metadata and
  syntax-only code units in generation-scoped SQLite. They may also store
  syntax-origin framework-role facts for recognized Express, React, and
  Jest/Vitest code-unit shapes with `FRAMEWORK_HEURISTIC` certainty, plus
  exact-anchor Python `DATAFLOW_DERIVED` support facts only after the
  application-layer derivation gate validates canonical targets and one
  framework role. By default they do not launch a semantic worker. When
  `REPOGRAMMAR_TYPESCRIPT_WORKER`
  names an explicit worker executable, optional
  `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies a JSON argv vector.
  Accepted worker facts may be stored only through same-generation
  path/hash/range evidence validation. Source snippets and absolute paths must
  not be stored; family rows must still be treated as absent unless the
  EC-MVFI-lite support gate has enough compatible evidence.
- CLI and MCP family detail output defaults to compact mode and must not render
  evidence records unless `evidence` or `deep` mode is explicitly selected.
  All matched modes include metadata-only read plans with repo-relative paths,
  strict content hashes, byte ranges, and no source text. Current `deep` mode is
  metadata-only and must report that source snippets are not included until a
  safe source-span rendering contract exists.
- Evidence/deep output may report greedy selector coverage metadata. Stored
  family evidence must carry explicit `covered_claims` labels, and selectors
  must not infer claim coverage from free-text notes or record order. The
  current builder emits `canonical`, `support`, and one narrow Python
  `variation` label when exact-compatible framework-anchor support targets
  differ inside an already-ready family; broader variation or exception
  coverage stays missing until explicit model links exist.
- `index` and `sync` must acquire `.repogrammar/locks/index.lock` before
  discovery, source reads, generation preparation, validation, and activation.
  They must clean up partial lock metadata writes and only remove the lock
  bytes they wrote. `unlock --force --yes` may remove confirmed stale
  `index.lock` only; active, unknown, invalid, daemon, and SQLite locks must not
  be deleted.
- Status and doctor JSON must keep manifest schema version and storage schema
  version separate. Do not reintroduce ambiguous `schema_version` fields in
  status output or `doctor.checks`.
- Tree-sitter is the intended syntax technology, but AST types must stay in
  adapters and Tree-sitter facts are not final semantic truth.
- v0.1 official language scope is Python-first, focused on FastAPI, pytest,
  SQLAlchemy, and Pydantic. Existing TypeScript/JavaScript discovery, storage,
  worker, fixture, and query substrate is transitional and must not be
  described as the official v0.1 implementation target unless a later ADR
  changes scope.
- Python v0.1 work must use public parser/analyzer APIs, repo-local helpers,
  native platform features, and installed dependencies where they already solve
  the problem. Do not hand-roll a Python parser, whole-program call graph, or
  type inference engine.
- Python v0.1 analysis must follow the ADR-0012 selective cascade: CPython
  `ast`/`symtable`/`tomllib` as primary frontend, Tree-sitter fallback only,
  Pyrefly through public provider boundaries for plausible family candidates,
  Pyright only for claim-upgrading cross-checks, and RightTyper-style observed
  evidence only behind explicit opt-in.
- Python framework compatibility must use typed canonical identities and an
  explicit compatibility table. Do not infer FastAPI, pytest, Pydantic,
  SQLAlchemy, or TS/JS framework compatibility from framework-name substrings in
  fact text, paths, notes, targets, or assumptions.
  Local lookalikes such as `@client.get(...)`, user-defined `BaseModel`, or a
  user-defined SQLAlchemy-shaped `Base` must remain non-family evidence unless
  exact compatible framework evidence passes the current support gate.
- Cross-checked and observed Python certainty labels are planned only; do not
  emit or test them as current protocol/storage/CLI/MCP tokens until all those
  contracts are updated together.
- v0.1 CLI is pattern-family-first; CodeGraph-style graph navigation command
  names are not top-level commands.
- CodeGraph is a possible optional lower-layer provider. It is not a
  dependency, not a wrapper target, and not a source of independent
  pattern-family proof.
- The default v0.1 MCP surface is one `repogrammar_context` tool with explicit
  operations, while CLI remains multi-command.
- Anonymous telemetry and research trace collection are separate consent paths.
  Anonymous telemetry is disabled by default, allowlist-validated, explicit
  upload only, and blocked by `REPOGRAMMAR_TELEMETRY=0`, `DO_NOT_TRACK=1`, or
  CI. Anonymous payloads must not contain repository instance ids, repository
  root hashes, paths, symbols, content hashes, byte ranges, raw targets, source,
  prompts, query text, patches, diffs, env vars, credentials, or raw errors.
  Telemetry export is inspect-only, and experiment export is redacted by
  default.
  Local stats must not report measured token savings without comparable paired
  baseline/treatment token measurements and a measurement source.
- Static uncertainty must be represented as typed `UNKNOWN`; dynamic behavior,
  conflicting facts, stale evidence, and insufficient support must not be
  guessed away.
- Syntax-origin framework-role facts must not be upgraded into resolved
  framework semantics, family evidence, conformance results, or query success
  without stronger compatible evidence and family claim builders. Raw Python
  parser facts remain structural; only separately synthesized exact-anchor
  support or future provider-backed facts can become family support.

## Implications

When adding automation, implement it under `src/` instead of adding root
scripts. Rust remains the core implementation language, while language-native
workers may use their native runtime behind protocol boundaries. When adding
analysis behavior, keep domain claims evidence-backed.

## Revalidation conditions

Revalidate after any accepted ADR changes source layout, external services,
parser strategy, optional provider strategy, language support, or uncertainty
policy.
