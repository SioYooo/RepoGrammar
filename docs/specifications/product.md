# Product Specification

RepoGrammar is a local tool for helping coding agents understand recurring
implementation patterns inside a repository.

## Product goal

RepoGrammar should return pattern-family evidence rather than only call graphs
or similarity search results. A result should be able to describe:

- common implementation skeletons;
- high-support repository conventions;
- legitimate variation slots;
- exceptions and counterexamples;
- closest matching implementations;
- contrastive examples that cover key differences;
- source evidence for every conclusion;
- `UNKNOWN` when static analysis cannot support a claim.

The v0.1 technical narrative is Evidence-Constrained Multi-View Family
Induction (EC-MVFI): syntax, compiler semantics, framework role, CFG/dataflow
and effect, API usage, and repository-context views may propose and validate
families, but claims are emitted only when evidence and compatibility gates
support them. Otherwise the result must remain `UNKNOWN`.

The CLI and MCP surfaces must preserve this identity. Human-facing commands are
organized around implementation-pattern families, not generic symbol graph
navigation.

## Intended users

- Local coding agents preparing implementation changes.
- Maintainers reviewing whether a proposed change matches repository norms.
- Developers seeking representative examples inside a large codebase.

## MVP scope

RepoGrammar v0.1 is Python-first. The official v0.1 implementation target is
local Python analysis for recurring repository pattern families in:

- FastAPI;
- pytest;
- SQLAlchemy;
- Pydantic.

The v0.1 product claim is: RepoGrammar provides sound-by-abstention,
metadata-only, repo-local Python implementation/integration-family evidence and
read planning for FastAPI, pytest, Pydantic, and SQLAlchemy. It can reduce
coding-agent context acquisition cost when local repeated patterns exist, and it
returns typed `UNKNOWN` when evidence is insufficient. This is not a claim of
sound Python semantic analysis.

The first Python implementation phase follows the claim-driven selective
cascade in `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`.
The implemented slice covers CPython `ast` structural candidates, path-derived
module anchors, CPython `symtable` structural scope anchors, private
`tomllib` project-config summaries, semantic-worker-compatible project-mode
module-level repo-local import resolution, default parser-mode repo-local
import context from discovered `.py` inventory and sanitized root
`pyproject.toml` source roots, and framework-role heuristics only, plus
file-local simple FastAPI router/app alias propagation and a narrow bounded
exact-anchor derivation step that synthesizes separate
`DATAFLOW_DERIVED` support facts when validated parser anchors exact-match the
canonical Python framework compatibility table for a unit with one framework
role. Product smoke tests now prove low-support and dynamic cases remain
`UNKNOWN`, while three-member direct FastAPI, FastAPI alias, pytest, Pydantic
model/settings, SQLAlchemy model-field, and SQLAlchemy session/repository
fixtures can produce families without a semantic worker through those derived
facts. Before storage, Python families now pass bounded complete-link
clustering over support-family features, preventing bridge members from
single-linking incompatible support into one confident claim. Ready Python
families can also record narrow variation metadata when their
already-compatible exact framework-anchor support targets differ within the same
family; this does not imply provider-backed semantics or runtime equivalence.
The current Python worker can also emit bounded same-function
FastAPI route service-call anchors as structural handler/service context; those
anchors are not membership support. It can also emit static FastAPI request
body and request-parameter anchors for `Body`, `Path`, `Query`, `Header`, and
`Cookie` marker shapes; those are route-shape context only and are not
membership support. Dynamic decorator factories and `setattr(...)`
monkey-patching become typed `UNKNOWN`s rather than inferred framework identity
or call-target evidence. It also persists root
`pyproject.toml` only as structural project-config context or typed config
`UNKNOWN`. Subsequent
slices should add selective Pyrefly provider queries for plausible family
candidates, Pyright cross-checks only for claim-upgrading facts, broader
bounded role propagation, cross-function target-centered call recovery, richer
EC-MVFI-lite family induction, and typed `UNKNOWN` governance.
The current Rust code now has only the owned future Python provider port
contract for request scope, provenance assumptions, cache keys, and recoverable
provider-unavailable `UNKNOWN`s, plus an application-layer planner that can
construct candidate-scoped Pyrefly framework-identity request envelopes for
future adapters from in-memory facts or validated active-generation snapshots.
It does not execute provider tools, store provider facts, or add production
provider-backed Python semantics.
The canonical algorithm contract is
`docs/specifications/python-analysis.md`.

Existing TypeScript/JavaScript discovery, syntax extraction, framework-role
facts, TypeScript worker protocol scaffolding, and release fixtures are
transitional substrate from the earlier bootstrap. They may remain useful, but
they are no longer the official v0.1 language target. The v0.2 preview may emit
conservative exact-anchor families for Express, Jest/Vitest, Next.js, Fastify,
Prisma, and Drizzle only when there are at least three complete-link-compatible
derived support facts and no claim-relevant blocking `UNKNOWN`s. Bounded TS/JS
project inventory may record package/config test-runner context, Next package
context, and JSON path aliases, and the syntax parser may record unique
repo-local literal relative/path-alias imports as `STRUCTURAL` context. Dynamic
imports, non-literal or conditional `require`, unresolved aliases/imports,
ambiguous re-exports, dynamic framework magic, raw query builders, and missing
project context remain typed `UNKNOWN` rather than family support.
Jest/Vitest script configs such as `jest.config.ts` or `vitest.config.js` are
recorded as metadata/typed `UNKNOWN` only; they are not executed and do not by
themselves provide ambient runner context. React components/hooks remain blocked
from public family claims even if an explicit TypeScript semantic-worker fact
names `react`. Production-quality TS/JS semantic analysis, React family support,
TypeScript compiler-backed evidence, complete re-export/path-alias semantics,
Fastify plugin-prefix resolution, Prisma/Drizzle runtime extensions, Next
server/client semantics, and dynamic-wrapper support remain deferred unless a
later ADR changes the sequence again.

Rust support in the v0.2 preview is limited to RepoGrammar self-dogfooding. It
uses Tree-sitter Rust for structural code-unit extraction and RepoGrammar-owned
internal role anchors. It may produce bounded internal families for
RepoGrammar's own indexing phases, family gates, parser adapters, CLI/MCP
handlers, installer actions, storage validation, source-span renderers, and
product tests when support is sufficient and no Rust-specific `UNKNOWN` blocks
the claim. It must not be described as general Rust semantic analysis: the
indexer does not run Cargo, rustc, build scripts, procedural macros, or
whole-program trait/call resolution.

## Public-preview support matrix

| Area | Status | Public claim |
|---|---|---|
| Python FastAPI | Supported | Bounded framework-family evidence under sound-by-abstention gates. |
| Python pytest | Supported | Bounded test/fixture family evidence with typed fixture ambiguity `UNKNOWN`. |
| Python Pydantic | Supported | Bounded model/settings family evidence; dynamic factories remain `UNKNOWN`. |
| Python SQLAlchemy | Supported | Bounded model/repository evidence; dynamic declarative patterns remain conservative. |
| JS/TS Express | Conservative v0.2 preview | Exact import/require plus direct literal route calls; support>=3 and complete-link compatibility required. |
| JS/TS Jest/Vitest | Conservative v0.2 preview | Exact imported/aliased runners or ambient test-file runners with safe project context; support>=3 required. |
| JS/TS Next.js | Structural v0.2 preview | `next` package context plus exact local App Router pages/layouts/routes and Pages Router pages/API routes; route groups, dynamic segments, middleware, server/client semantics, server actions, and re-exported routes remain `UNKNOWN`. |
| JS/TS Fastify | Structural v0.2 preview | Exact local Fastify factory receiver plus shorthand or literal `app.route` declarations; dynamic methods/options, plugin registration, and prefix semantics remain `UNKNOWN`. |
| JS/TS Prisma | Structural v0.2 preview | Exact local `new PrismaClient()` bindings plus whitelisted model operations and array `$transaction`; injected clients, extensions, callback transactions, dynamic model/op access, and raw SQL remain `UNKNOWN`. |
| JS/TS Drizzle | Structural v0.2 preview | Exact Drizzle table factories, local `drizzle(...)` db bindings, and whitelisted query builders; unresolved tables/dbs, dynamic builders, and raw SQL remain `UNKNOWN`. |
| JS/TS React | Not supported | Components/hooks may be detected as roles but cannot form public family claims. |
| Full JS/TS semantics | Not supported | No compiler-backed TypeScript analysis, full alias/re-export semantics, runtime DI, or dynamic wrapper execution. |
| Rust self-dogfood | Internal v0.2 preview | RepoGrammar-owned implementation-family evidence only; Tree-sitter structural anchors with no Cargo/rustc/proc-macro execution. |
| Source snippets | Explicit opt-in only | Default output is metadata-only; bounded source spans require explicit CLI/MCP opt-in and hash checks. |
| Token savings | Not claimed by default | Only paired baseline/treatment experiments may report measured savings. |

Django, C/C++, whole-program Python call graphs, sound full Python semantic
analysis, and default runtime tracing are deferred.

## Non-goals

- No cloud service dependency.
- No local LLM, embedding model, vector database, or remote API.
- No global database for repository-derived family facts, evidence, source
  hashes, freshness metadata, or repository paths.
- No automatic modification of user business code from pattern-family results.
- No top-level v0.1 `callers`, `callees`, `impact`, `affected`, `node`, or
  `explore` commands.
- No production-readiness claims outside the scoped v0.1 Python family/read-plan
  contract, and no token-savings claims until measured evidence exists.
- No mandatory CodeGraph dependency. CodeGraph may be considered only as an
  optional lower-layer provider, not as RepoGrammar's product identity.

## Result discipline

RepoGrammar must distinguish `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, and
`UNKNOWN`. Low confidence, competing families, incompatible targets, and dynamic
runtime behavior must lead to abstention rather than certainty.

Structural similarity may generate candidates, but it must not by itself prove
semantic family membership. Language-native semantic facts take precedence over
framework heuristics and syntax-only fingerprints. Syntax-origin framework-role
facts can record that a code unit has a recognizable framework role shape, but
`FRAMEWORK_HEURISTIC` certainty is not enough to prove family membership,
resolved handler identity, runtime lifecycle equivalence, or conformance.
Freshness is a required gate before semantic facts can become inputs to future
family claim builders. A fresh supported fact kind is still only eligible input;
it is not a `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, or conformance result
until EC-MVFI support, compatibility, and contrastive evidence checks are
implemented.
The current EC-MVFI-lite implementation is deliberately narrow: it can only
store a `DOMINANT_PATTERN` family when repeated compatible framework-role
candidates also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
non-framework evidence. That support must be role-compatible: an arbitrary
semantic fact for an unrelated package, API, or framework cannot prove an
FastAPI, pytest, SQLAlchemy, Pydantic, Express, Jest/Vitest, Next.js, Fastify,
Prisma, or Drizzle family. React components/hooks are currently recognized only
as syntax/framework-role shapes and stay `UNKNOWN`.
Otherwise family queries must return typed `UNKNOWN` rather than upgrading
syntax/framework heuristics into claims.
Family query output is selected rather than dumped wholesale. The default
compact mode returns family summary, members, variation slots, and unknowns
without evidence records or source snippets. All matched family modes return a
read plan that tells an agent which target, canonical, support, and
variation/exception spans to inspect by repo-relative path, strict content
hash, and byte range. The read plan reduces blind line-range expansion when
graph/navigation tools omit key function bodies, but it does not eliminate the
requirement to read target source before editing outside rendered ranges.
Explicit evidence/deep modes may return selected repo-relative evidence
metadata under a token budget. Explicit source-span opt-in may return bounded,
line-numbered, hash-checked spans selected from the read plan; stale or
unsupported spans are omitted with fallback guidance. The current selector uses
greedy marginal coverage over conservative metadata labels and reports missing
requested coverage instead of inventing unsupported variation or exception
evidence. The only current variation evidence link is
Python exact-compatible framework-anchor target diversity inside an already
ready family; exception evidence remains deferred. Deep mode remains
metadata-first unless source spans are explicitly requested.

`repogrammar stats --json` reports repo-shape diagnostics for local pattern
density, family support coverage, abstention rate, and thin-wrapper/token-saving
risk. These diagnostics explain when RepoGrammar can reduce context acquisition
cost and when third-party-heavy or thin-wrapper repositories are unlikely to
produce large savings. They are not measured token savings or causal claims.
Measured token savings are reported only when a local paired baseline/treatment
token experiment has comparable token counts and a measurement source.

`UNKNOWN` is a typed result with reason codes and affected claims, not an
implementation failure by default. Some unknowns block specific semantic,
security, persistence, or conformance claims while still allowing weaker
structural observations. The canonical taxonomy lives in
`docs/specifications/unknowns.md`.

## Installation and telemetry boundaries

Machine-level `install` and `uninstall` are separate from repository-level
`init`, `index`, and `sync`. Installer behavior must be reversible, scoped, and
dry-run friendly.

End-user installation must be binary-first. Public-preview users should be able
to install and run the RepoGrammar CLI from a prebuilt release artifact without
Rust, Cargo, Node.js, npm, Docker, the SQLite CLI, local LLMs, embedding models,
or cloud API keys. Rust/Cargo remains a contributor and source-build dependency
only. The current Python preview still requires a `python3` interpreter at
indexing time for the bundled CPython AST worker asset; it must not require a
Python virtualenv or project dependency installation.
The npm package is an optional thin launcher for users who already have
Node/npm; it downloads and execs the same release binary and must not become a
JavaScript reimplementation of RepoGrammar.

Repository-derived analysis state belongs in the current repository's
`.repogrammar/` state directory, or the directory named by `REPOGRAMMAR_DIR`.
Global user state may contain installation receipts, binary/cache metadata,
anonymous telemetry preference, anonymous machine id, and non-repository-derived
runtime artifacts only.

Anonymous telemetry and research trace collection are separate consent
decisions. Anonymous telemetry is disabled by default, upload is explicit, and
environment opt-outs prevent network upload. Context compression metrics are not
actual token savings unless a comparable baseline and treatment token
measurement exist.
Live install keeps telemetry consent independent from agent configuration:
`--yes` never implies telemetry consent, `--telemetry` and `--no-telemetry` are
the explicit non-interactive choices, and product interactive installs prompt
with default-no `[y/N]` when no telemetry flag is supplied. Enabled
`stats --json` may update only a bucketed repo-local passive diagnostics
rollup; network upload remains limited to explicit `repogrammar telemetry
upload`.
Anonymous telemetry payloads must not include a repository instance id,
repository root hash, source path, symbol, content hash, byte range, raw target,
prompt, source snippet, or raw error. Experiment export is redacted by default
and reports token/count data only through coarse buckets.

RepoGrammar v0.1 first-class coding-agent integrations are Claude Code and
Codex. Both integrations use the same read-only `repogrammar_context` MCP
server through native agent CLI commands and RepoGrammar-owned receipts.
Interactive `repogrammar install` is a machine-level TUI-style wizard that can
wire Codex, Claude Code, or both in one run, skip already managed agents, and
add missing supported agents later. Multi-agent live install is
all-or-rollback, and `--target all --scope global --yes` uses that same
transaction. The installer may place the `repogrammar` command in a
user-writable command directory, but it must not index code, mutate
`.repogrammar/`, edit instruction files, upload telemetry, or run paired
experiments. Project-local live writes and instruction-file edits remain
deferred unless separately specified and tested. The install target registry may
recognize additional CodeGraph-style agents for dry-run and `--print-config`
planning, but recognized target ids are not live support claims until the
adapter has a reversible writer, ownership receipt, uninstall inverse, and
default tests.
