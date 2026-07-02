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
The current Rust code now has owned future Python, Rust, and TS/JS provider
port contracts for request scope, provenance assumptions, cache keys, and
recoverable provider-unavailable `UNKNOWN`s, plus an application-layer planner
that can construct candidate-scoped Pyrefly framework-identity request
envelopes for future adapters from in-memory facts or validated
active-generation snapshots. Apart from the Rust Cargo metadata project-model
stage described below, it does not execute provider tools, store provider facts,
or add production provider-backed Python, Rust, or TS/JS family support.
The default product indexing path now wires the Rust Cargo metadata adapter as a
safe project-model refresh stage when same-generation `Cargo.toml` code units
exist. It can produce owned `PROJECT_CONFIG` facts and provider `UNKNOWN`s
without executing build scripts or procedural macros, but it does not prove
symbol/type/call semantics or family support. The Rust/TSJS semantic analysis
roadmap is tracked in `docs/plans/rust-tsjs-semantic-analysis-plan.md`.
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
context, JSON path aliases, and safe JSON `rootDirs`, and the syntax parser may
record unique repo-local literal relative/path-alias/rootDirs imports as
`STRUCTURAL` context. Dynamic imports, non-literal or conditional `require`,
unresolved aliases/imports/rootDirs targets, ambiguous re-exports, dynamic
framework magic, raw query builders, and missing project context remain typed
`UNKNOWN` rather than family support.
Exact TS/JS framework bindings may use ES imports, CommonJS `require`, or
CommonJS destructuring aliases from the exact supported framework package; this
does not support custom wrappers, injected clients, plugin containers, or
runtime extension mechanisms.
Jest/Vitest script configs such as `jest.config.ts` or `vitest.config.js` are
recorded as metadata/typed `UNKNOWN` only; they are not executed and do not by
themselves provide ambient runner context. React components/hooks remain blocked
from public family claims even if an explicit TypeScript semantic-worker fact
names `react`. Production-quality TS/JS semantic analysis, React family support,
complete Program/TypeChecker coverage, complete re-export/path-alias semantics,
Fastify plugin-prefix resolution, Prisma/Drizzle runtime extensions, Next
server/client semantics, middleware, server actions, re-export semantics, and
dynamic-wrapper support remain deferred unless a later ADR changes the sequence
again. A bounded optional TypeScript worker operation slice may produce
compiler-backed module-resolution facts and may cross-check exact Next.js
file-convention export identity, relative repo-local Express/Fastify named
handler imports, relative repo-local Prisma shared-client bindings, or relative
repo-local Drizzle db/table bindings when a TypeScript compiler API is
available. Only TypeScript-provider
`resolve_export` facts that match the parser Next.js anchor's
path/hash/code-unit/range, export name, framework role, and structural-anchor
provenance, or TypeScript-provider `resolve_reexport` facts that match a
provider-required Express/Fastify handler or Prisma anchor's
path/hash/code-unit/range and named relative import such as
`./handlers#listUsers` or `./db#prisma`, or every required Drizzle named
import such as `./db#db` and `./schema#users`, may become TS/JS-derived
support.
Static fallback facts remain structural context and do not support family claims. Exact local
Next dynamic segments, route groups, and parallel routes may be recorded as
context assumptions on supported page/layout/route anchors; they are not
independent support evidence.

Rust support in the v0.2 preview is limited to RepoGrammar self-dogfooding. It
uses Tree-sitter Rust for structural code-unit extraction and RepoGrammar-owned
internal role anchors. It may produce bounded internal families for
RepoGrammar's own indexing phases, family gates, parser adapters, CLI/MCP
handlers, installer actions, storage validation, source-span renderers, and
product tests when support is sufficient and no Rust-specific `UNKNOWN` blocks
the claim. It must not be described as general Rust semantic analysis: the
indexer does not run Cargo, rustc, build scripts, procedural macros, or
whole-program trait/call resolution. Cargo build scripts and target-specific
root manifest sections are typed build-variant `UNKNOWN`s that block affected
Rust self-dogfood families until resolved. Nested fixture/package manifests
must not globally block unrelated root Rust family support. Source-level
`#[cfg]` and `#[cfg_attr]` build-variant UNKNOWNs may carry bounded nearest
`Cargo.toml` feature context, including simple feature predicates and whether
the feature is declared, but that context does not evaluate cfgs or prove
family support.

Java support in the v0.2 preview is a conservative Spring/Spring Boot
structural slice. RepoGrammar can discover `.java` files, parse Java classes,
interfaces, methods, and Spring-shaped units with Tree-sitter Java, and derive
bounded support for exact source-visible Spring annotations only when the
annotation is fully qualified or imported from the expected Spring package.
The supported Spring targets are Spring MVC `@RequestMapping` /
`@GetMapping` / `@PostMapping` / `@PutMapping` / `@PatchMapping` /
`@DeleteMapping`, Spring stereotypes including `@Controller`,
`@RestController`, `@Service`, `@Repository`, and `@Component`,
`@SpringBootApplication`, and Spring Data repository interfaces that exactly
extend/import `JpaRepository` or use exact `@RepositoryDefinition`. The parser
does not execute Maven, Gradle, javac, annotation processors, Spring component
scans, dependency injection, AOP proxies, repository factories, or classpath
resolution. Lookalike simple annotations without exact imports, custom composed
annotations, route constants, DI/proxy semantics, missing project context, and
classpath-sensitive behavior remain typed `UNKNOWN` or non-supporting context.
Java/Spring families require at least three complete-link-compatible derived
support facts and no claim-relevant blocking `UNKNOWN`.

## Public-preview support matrix

| Area | Status | Public claim |
|---|---|---|
| Python FastAPI | Supported | Bounded framework-family evidence under sound-by-abstention gates. |
| Python pytest | Supported | Bounded test/fixture family evidence with typed fixture ambiguity `UNKNOWN`. |
| Python Pydantic | Supported | Bounded model/settings family evidence; dynamic factories remain `UNKNOWN`. |
| Python SQLAlchemy | Supported | Bounded model/repository evidence; dynamic declarative patterns remain conservative. |
| JS/TS Express | Conservative v0.2 preview | Exact import/require bindings, including CommonJS destructuring aliases, plus direct literal route calls; support>=3 and complete-link compatibility required. |
| JS/TS Jest/Vitest | Conservative v0.2 preview | Exact imported/aliased runners or ambient test-file runners with safe project context; support>=3 required. |
| JS/TS Next.js | Compiler-cross-checked v0.2 preview | `next` package context plus exact local App Router pages/layouts/routes and Pages Router pages/API routes; configured TypeScript workers can cross-check exact route/page/layout/API export identity, while dynamic segments, route groups, parallel routes, middleware, server/client semantics, server actions, and re-exported routes remain context or `UNKNOWN`. |
| JS/TS Fastify | Structural v0.2 preview | Exact local Fastify factory receiver, including CommonJS destructuring aliases, plus shorthand or literal `app.route` declarations; dynamic methods/options, plugin registration, and prefix semantics remain `UNKNOWN`. |
| JS/TS Prisma | Compiler-cross-checked v0.2 preview | Exact local `new PrismaClient()` bindings, including CommonJS destructuring aliases, plus whitelisted model read/write operations and array `$transaction`; configured TypeScript workers can cross-check relative repo-local named shared-client imports, while bulk operations, injected clients, external shared clients, extensions, callback transactions, dynamic model/op access, and raw SQL remain `UNKNOWN`. |
| JS/TS Drizzle | Compiler-cross-checked v0.2 preview | Exact Drizzle table factories and local `drizzle(...)` db bindings, including CommonJS destructuring aliases, remain structural; configured TypeScript workers can cross-check relative repo-local named db/table imports for whitelisted `select`/`insert`/`update`/`delete` and `db.query.<table>.findMany/findFirst`; unresolved tables/dbs, external imports, dynamic builders, and raw SQL remain `UNKNOWN`. |
| JS/TS React | Not supported | Components/hooks may be detected as roles but cannot form public family claims. |
| Full JS/TS semantics | Not supported | Only bounded optional TypeScript worker operations exist; no full Program/TypeChecker semantics, runtime DI, dynamic wrapper execution, or broad JS/TS family support. |
| Rust self-dogfood | Internal v0.2 preview | RepoGrammar-owned implementation-family evidence only; Tree-sitter structural anchors with no Cargo/rustc/proc-macro execution. |
| Rust provider-backed project model | Preview | Default indexing can refresh Cargo metadata `PROJECT_CONFIG` facts for discovered manifests without build-script/proc-macro execution, and parser-origin cfg UNKNOWNs can carry bounded Cargo feature context; rust-analyzer/rustc/rustdoc JSON adapters and provider-backed family support remain deferred. |
| Java Spring MVC | Structural v0.2 preview | Exact imported/FQN Spring MVC route annotations inside exact controllers; route constants and runtime dispatch remain typed `UNKNOWN` subclaims. |
| Java Spring/Spring Boot components | Structural v0.2 preview | Exact Spring stereotypes and `@SpringBootApplication`; component scan, DI, auto-configuration, and proxy behavior remain runtime `UNKNOWN`s. |
| Java Spring Data | Structural v0.2 preview | Exact imported/FQN `JpaRepository` inheritance or `@RepositoryDefinition`; generated implementations, repository factories, module selection, and classpath resolution remain `UNKNOWN`. |
| TS/JS provider-backed semantics | Limited preview | Bounded TypeScript worker export/binding facts can support exact Next.js file-convention export identity, relative repo-local Express/Fastify named handler imports, relative repo-local Prisma shared-client binding, and relative repo-local Drizzle db/table bindings after path/hash/code-unit/range/role validation; TypeScript Program/TypeChecker, Language Service, CodeQL, abstract-analysis workers, and broad JS/TS provider-backed families remain deferred. |
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
Prisma, Drizzle, or Java/Spring family. React components/hooks are currently
recognized only as syntax/framework-role shapes and stay `UNKNOWN`.
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

`repogrammar stats --json` reports Python-family repo-shape diagnostics for
local pattern density, family support coverage, abstention rate, and
thin-wrapper/token-saving risk, plus token-saving readiness and concrete
blocking reasons. The JSON includes
`repo_shape_scope: python_family_eligible_units` so these readiness diagnostics
are not confused with multi-language inventory. These diagnostics explain when
RepoGrammar can reduce context acquisition cost and when third-party-heavy or
thin-wrapper repositories are unlikely to produce large savings. They are not
measured token savings or causal claims. Measured token savings are reported
only when a local paired baseline/treatment token experiment has comparable
token counts and a measurement source; otherwise stats must mark the
measurement kind as `ESTIMATED` and include a not-measured caveat.

`UNKNOWN` is a typed result with reason codes and affected claims, not an
implementation failure by default. Some unknowns block specific semantic,
security, persistence, or conformance claims while still allowing weaker
structural observations. The canonical taxonomy lives in
`docs/specifications/unknowns.md`. `repogrammar unknowns --json` and
`repogrammar stats --unknowns --json` expose source-free aggregate inventory for
persisted semantic `UNKNOWN` facts. The inventory reports
`inventory_scope: persisted_semantic_unknowns`, stable recovery-code buckets,
role-state buckets, mechanism buckets, and support-blocking buckets for
prioritizing provider and analyzer work; reductions in those counts are
diagnostic only unless false certainty is also controlled.

## Installation and telemetry boundaries

Machine-level `install` and `uninstall` are separate from repository-level
`init`, `resync`, `autosync`, and `uninit`. Installer behavior must be
reversible, scoped, and dry-run friendly. Repository bootstrap is explicit:
`repogrammar init --yes --resync --autosync` creates repo-local analysis state,
rebuilds the active generation, and starts auto-sync only when the user or
agent has permission to do so.

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
