# Domain Model Specification

This specification defines the initial domain vocabulary. The Rust-core bootstrap
implements only minimal types and placeholders.

## CodeUnit

A `CodeUnit` is a repository-owned analyzable source unit such as a function,
class, module, or test case. It carries a language, kind, source range, and
provenance. It must not contain Tree-sitter nodes or transport-specific types.

Current syntax-only indexing can persist transitional TS/JS `CodeUnit` records
for modules, functions, assigned arrow functions, classes, methods, React
function components, custom hooks, Express route calls, Next.js App/Pages
Router conventions, Fastify route declarations and plugin registrations, Prisma
query/transaction calls, Drizzle schema/query/transaction anchors, Zod schema
builders (`zod_schema`), NestJS controllers/routes/injectables/modules
(`nest_controller`, `nest_route`, `nest_injectable`, `nest_module`), Hono routes
(`hono_route`), and Jest/Vitest suite or test blocks (also aliased for Mocha and
`node:test`).
The first Python v0.1 slice can also persist CPython
`ast`-derived records for modules, functions, async functions, classes, methods,
FastAPI route-shaped functions, pytest tests and fixtures, Pydantic model-shaped
classes, SQLAlchemy model-shaped classes, and SQLAlchemy repository
method-shaped functions. Root `pyproject.toml`, `setup.cfg`, and `setup.py` may
be represented as `python-config` language files with `project_config` code
units so each config artifact shares the same generation, hash, and evidence
validation boundary. Their structural parser methods are `tomllib`,
`configparser`, and `cpython_ast`; `setup.py` is parsed, never executed.
Go discovery defines stable `go` and `go-config` language tokens so `.go` and
root/nested `go.mod`/`go.work` file records can be persisted source-free. Those
tokens are inventory-only in the current product: source reads and parsing are
skipped, and no Go `CodeUnit`, IR, semantic fact, or family is created. The
token's presence is not support evidence. Go-only and empty generations are
`file_manifest_only`; a mixed generation remains `syntax_only_code_units`
because its non-Go parser-capable tokens still own code-unit semantics.
Ruby discovery likewise defines stable `ruby` and `ruby-config` tokens for
bounded `.rb` and accepted root/nested project/configuration paths. They are
inventory-only: only repository-relative path, strict hash, byte size, and token
are persisted; no Ruby `CodeUnit`, IR, semantic fact, typed `UNKNOWN`, or family
exists. Ruby-only generations are `file_manifest_only`, while a mixed generation
remains `syntax_only_code_units`. The tokens prove file inventory only and do
not select a Ruby engine, project root, dependency graph, or support state.
PHP discovery defines stable `php` and `php-config` tokens for exact `.php`
paths and exact accepted Composer/PHPUnit configuration basenames. They are
inventory-only: only repository-relative path, strict raw-byte hash, byte size,
and token are persisted; no PHP `CodeUnit`, IR, semantic fact, typed `UNKNOWN`,
or family exists. PHP-only generations are `file_manifest_only`; mixed
generations remain `syntax_only_code_units`. The tokens prove file inventory
only and do not select a PHP profile, Composer project, PHPUnit version,
dependency graph, custom vendor directory, or support state.
Swift discovery defines stable `swift` and `swift-config` tokens for exact
`.swift` paths and exact accepted SwiftPM/toolchain-selector basenames. They are
inventory-only: only bounded repository-relative path, strict raw-byte hash,
byte size, and token are persisted; no Swift `CodeUnit`, IR, semantic fact,
typed `UNKNOWN`, project model, or family exists. Swift-only generations are
`file_manifest_only`; mixed generations remain `syntax_only_code_units`. The
tokens prove file inventory only and do not select a manifest, package target,
toolchain, SDK, XCTest identity, dependency graph, or support state.
The Java/Spring v0.2 preview can persist Tree-sitter Java structural records for
Java classes/interfaces/methods plus Spring MVC route methods, Spring
components, Spring Boot applications, and Spring Data repositories when exact
source-visible Spring annotations or repository types are present. Wave J1 adds
`junit5_test_method`, `junit4_test_method`, `testng_test_method`, `jpa_entity`,
`jpa_mapped_superclass`, `jpa_embeddable`, `jaxrs_resource_class`, and
`jaxrs_resource_method` unit kinds (entities/resource classes map to Class-like
IR nodes; test/resource methods to Method-like) for exact imported/FQN JUnit,
TestNG, JPA (dual `jakarta`/`javax` roots), and JAX-RS anchors. The C# v0.2
preview can persist Tree-sitter C# structural records for classes, records,
structs, interfaces, methods, and properties plus ASP.NET Core controllers and
controller actions, minimal-API routes, EF Core `DbContext`/`DbSet` entity sets,
and xUnit/NUnit/MSTest test methods when exact using/FQN-gated framework anchors
are present. The C/C++ v0.2 preview can persist Tree-sitter C/C++ structural
records for modules, classes/structs, and functions plus GoogleTest test cases
and fixtures, Catch2/doctest test cases, and Boost.Test cases and suites when
include-evidence-gated registration-macro shapes are present, and `cpp-config`
`PROJECT_CONFIG` records for `compile_commands.json`/`vcpkg.json`/`conanfile.txt`.
The Rust v0.2 preview persists Tree-sitter Rust structural records
for modules, structs, enums, traits, impl blocks, functions, methods, and tests,
plus RepoGrammar self-dogfood roles and — in any repository — general framework
kinds `serde_model`, `thiserror_error_enum`, `tokio_entry`, `tokio_test`,
`clap_parser`, and `axum_route` when the exact serde/thiserror/tokio/clap derive
or attribute shape or an axum literal `Router::new().route(...)` segment is
backed by same-file `use`-path evidence or an inline fully-qualified path.
These records are structural candidates only. They are
not semantic facts, resolved symbols, framework-equivalence claims, or
pattern-family membership evidence. A separate syntax-origin `SemanticFact` may
be derived from some framework-shaped code units, but that fact remains
framework-heuristic evidence for future claim builders, not proof of semantic
equivalence or family membership.

Because these are structural heuristics, candidate classification must stay
conservative and precise rather than over-claim from loose substring matches:
the Next.js Pages Router convention is recognized under both `pages/` and
`src/pages/` (mirroring `app/`/`src/app/`); an assigned arrow-function unit is
recorded only when the right-hand side actually begins an arrow function, not
when a `=>` merely appears in a callback argument; and a Rust `impl` function is
classified as a method only when its parameter list carries a `self` receiver,
not when the token `self` appears elsewhere in its body.

## Unified IR

The unified IR is a lightweight RepoGrammar representation derived from parser
and semantic-worker output. It supports structural comparison while hiding
parser-specific AST, compiler, LSP, and SDK details from the core domain.

Current syntax-only indexing persists only the bootstrap IR subset: one IR node
per code unit and conservative `contains` edges from modules to contained units
and from classes to methods. IR node payloads remain `{}` until typed IR
attributes are designed. This structural IR is not semantic certainty or
pattern-family membership evidence.

## SemanticFact

A `SemanticFact` records language-native or derived information such as resolved
calls, imports, symbols, types, or framework roles. Facts include origin,
certainty, evidence, and assumptions.

Certainty levels are semantic, dataflow-derived, structural,
framework-heuristic, conflicting, and unknown. Structural certainty is not
enough to prove family membership. Framework-heuristic certainty is also not
enough to prove family membership.
The current Rust domain includes an internal claim-input readiness gate for
semantic facts: fresh supported fact kinds with `SEMANTIC` or
`DATAFLOW_DERIVED` certainty may become inputs to future family claim builders,
while stale evidence, conflicting facts, structural certainty, framework
heuristics, unknown certainty, and `UNKNOWN` fact kind are blocked with typed
`UNKNOWN`. Readiness is not itself a family classification.
Current default TS/JS indexing can store syntax-origin `FRAMEWORK_ROLE` facts
for recognized Express, React, Jest/Vitest (with Mocha/`node:test`
`runner_kind`), Zod, NestJS, Hono, Next.js, Fastify, Prisma, and
Drizzle code-unit shapes. These facts carry repo-relative code-unit evidence,
`FRAMEWORK_HEURISTIC` certainty, and explicit unresolved-binding assumptions;
they do not resolve TypeScript symbols, framework runtime behavior, or family
membership.
The TS/JS syntax parser can also persist `STRUCTURAL` exact-anchor facts for
bounded Express, Jest/Vitest, Next.js, Fastify, Prisma, and Drizzle shapes,
including exact ES import, CommonJS `require`, and CommonJS destructuring-alias
bindings from supported framework packages. It also persists typed `UNKNOWN`
facts for dynamic, unsafe, or unresolved
receiver/runner/support-target/framework-magic boundaries, and structural
project-config facts for bounded `package.json`, `tsconfig.json`,
`jsconfig.json`, Jest config, and Vitest config inventory, including safe JSON
path-alias and rootDirs metadata. Project-config facts remain context only.
Only application-layer `repogrammar-tsjs-derived`
`DATAFLOW_DERIVED` facts with exact whitelisted targets and framework-specific
`derived_from=tsjs_<framework>_structural_anchors` assumptions can support
conservative TS/JS families, and those families require repeated compatible
support rather than a single syntax match.
Current default Java indexing can store syntax-origin `FRAMEWORK_ROLE` facts
for Spring MVC routes, Spring components, Spring Boot applications, and Spring
Data repositories. The Java parser can also persist `STRUCTURAL` exact-anchor
facts for fully qualified or imported Spring annotations and repository types,
and typed `UNKNOWN` facts for unresolved imports, nonliteral route paths,
controller/repository identity uncertainty, and runtime framework behavior.
Only application-layer `repogrammar-java-derived` `DATAFLOW_DERIVED` facts with
exact whitelisted targets and `derived_from=tree_sitter_java_structural_anchors`
can support Java/Spring families. This does not prove Maven/Gradle, javac,
annotation-processor, classpath, component-scan, dependency-injection, proxy, or
repository-factory semantics.
Current default C# indexing can likewise store syntax-origin `FRAMEWORK_ROLE`
facts for ASP.NET Core controllers/actions, minimal-API routes, EF Core
contexts/entity sets, and xUnit/NUnit/MSTest tests, `STRUCTURAL` exact-anchor
facts for using/FQN-gated attributes, and typed `UNKNOWN` facts for unresolved
attribute bindings, controller/test-class identity, minimal-API receivers,
nonliteral route templates, build variants, and runtime framework behavior. Only
application-layer `repogrammar-csharp-derived` `DATAFLOW_DERIVED` facts with
exact whitelisted targets and
`derived_from=tree_sitter_csharp_structural_anchors` can support C# families.
This does not prove MSBuild, Roslyn, source-generator, ASP.NET Core runtime,
dependency-injection, or preprocessor-variant semantics.
Current default C/C++ indexing can likewise store syntax-origin `FRAMEWORK_ROLE`
facts for GoogleTest/Catch2/doctest/Boost.Test cases, fixtures, and suites,
`STRUCTURAL` exact-anchor facts for include-evidence-gated registration macros,
`PROJECT_CONFIG` facts for `compile_commands.json`/`vcpkg.json`/`conanfile.txt`
inventory, and typed `UNKNOWN` facts for unresolved framework identity,
Catch2-vs-doctest conflicts, build variants, macro boundaries, moc/generated
code, and dispatch. Only application-layer `repogrammar-cpp-derived`
`DATAFLOW_DERIVED` facts with exact whitelisted targets and
`derived_from=tree_sitter_c_cpp_structural_anchors` can support C/C++ families.
This does not prove build-system, compiler, preprocessor, macro-expansion,
moc/protoc, points-to, or class-hierarchy dispatch semantics.
Current default Python indexing can store syntax-origin `FRAMEWORK_ROLE` facts
for FastAPI route-shaped units, pytest tests/fixtures, Pydantic models,
SQLAlchemy models, and SQLAlchemy repository methods. These facts also use
`FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions; they do not
resolve imports, decorator targets, fixture bindings, SQLAlchemy mappings, or
family membership. Current default Python indexing can also persist CPython
`ast` parse-document structural facts for import bindings, decorator anchors,
class bases, simple calls, FastAPI static response-model/dependency-target/error
anchors including literal HTTPException status-code effect anchors, Pydantic
field, field-type, `model_config`, nested `Config`, computed-field, validator,
and model-validator anchors, bounded same-function FastAPI service-call anchors,
and typed
dynamic/unresolved `UNKNOWN` cases. These facts remain `STRUCTURAL` or
`UNKNOWN`, are blocked from support input, and may be fed to the current family
builder only as context features or claim-scoped blocking `UNKNOWN`s. A separate
application-layer derivation step may synthesize `DATAFLOW_DERIVED` support
facts from those validated structural anchors only when the unit has exactly
one Python framework role, evidence stays in the same code-unit path/hash/range,
and the target exact-matches the canonical Python compatibility table. Those
derived facts use `provider_resolved=false`; they are bounded support for
EC-MVFI-lite, not provider-backed semantic identity or runtime-equivalence
proof. It can also persist
`PROJECT_CONFIG` facts for sanitized root `pyproject.toml`, `setup.cfg`, or
`setup.py` project names and safe source roots, plus recognized tool sections
where available, or typed config `UNKNOWN`s when the selected parser is
unavailable for TOML or the input is malformed. A static `setup.py` call is
accepted only with zero positional arguments, no keyword unpacking, unique
relevant keywords, a complete unique string-to-string `package_dir`, and an
unambiguous literal package-finder `where`. Computed, incomplete, duplicate,
overridable, or top-level-unreachable recognized config yields
`MissingProjectConfig`; `setup()` remains valid empty config. Calls not
lexically traced to `setuptools`, conditional calls, standalone/lookalike
finders, and bindings/module aliases that may be shadowed, deleted, or explicitly
mutated (including through builtins-qualified helpers) abstain. Multiple
independently authoritative setup calls emit `ConflictingFacts` rather than
merging metadata.
`PROJECT_CONFIG` facts are structural context only and are blocked from claim-
input readiness even if a future bug marks them with stronger certainty.
Safe roots from coexisting Python config formats form only a deduplicated
structural candidate set. They do not encode packaging/setuptools precedence,
and config conflict/UNKNOWN records cannot prove or suppress a strong claim.
Pydantic member/config/computed anchors are
also structural model metadata only; they do not exact-match the support table
and cannot prove family membership. FastAPI service-call anchors are structural
handler/service context only and also cannot prove route-family membership.
Future Python facts should follow the
same owned model for FastAPI, pytest, SQLAlchemy, and Pydantic evidence; parser,
type-checker, LSP, or Python runtime objects must not enter the core domain.

Python framework compatibility must use typed canonical identities rather than
free-text matching. Python facts should map provider output into owned records
such as resolved symbol, subclass, decorator binding, call target, and fixture
binding facts with canonical fully qualified names. A framework claim must be
checked against an explicit compatibility table for FastAPI, pytest, Pydantic,
and SQLAlchemy, plus the ADR-0019 bounded preview roles for Django
(model/url-pattern/test), Flask, stdlib `unittest`, click/typer, and Celery. Do
not infer Python framework compatibility from substrings in fact kind, engine,
method, target, assumptions, path, or note fields.

Future Python provider states such as cross-checked static facts or observed
runtime facts require explicit domain/protocol/storage changes before becoming
public certainty tokens. Until then, cross-check status and observed provenance
must stay in owned assumptions/provenance and use the current certainty
vocabulary.
The current Rust ports layer defines owned Python, Rust, and TS/JS provider
request, provenance, cache-key, and unavailable-output types so future Pyrefly,
Pyright, RightTyper, Cargo/rust-analyzer/rustc/rustdoc JSON, TypeScript
Compiler API/Language Service, CodeQL, or abstract-analysis adapters can
translate into `SemanticFact` plus typed `UNKNOWN`. These port types are not
provider facts by themselves and do not change the current family support gate.
The Rust Tree-sitter parser can also attach bounded Cargo feature context to
source-level `#[cfg]` / `#[cfg_attr]` `UNKNOWN` facts by reading the nearest
discovered `Cargo.toml` from parser project context. Those assumptions may say
which simple feature predicates were seen and whether the feature is declared,
but they remain `UNKNOWN` metadata rather than cfg evaluation, symbol
resolution, or family evidence. The same bounded Rust path may record
structural Cargo package/edition/dependency/target/crate-root metadata and exact
repo-local `use crate::...`, `use super::...`, or `use self::...` import
context, but those facts remain structural context and do not prove general Rust
semantics or external dependency behavior.

## PatternFamily

A `PatternFamily` will group related code units that share an implementation
pattern. The current storage substrate can persist generation-scoped family
records, family members, variation slots, and family-bound evidence. The
current EC-MVFI-lite builder can populate those records only for repeated
compatible candidates backed by strong semantic/dataflow support; syntax-only
framework-role facts produce typed `UNKNOWN` instead of a family claim. Python
families also pass a bounded complete-link clustering step over support-family
features, so a bridge member cannot connect otherwise incompatible Python
support families. Full template induction and exception mining remain deferred.

## CanonicalTemplate

A `CanonicalTemplate` will represent the shared implementation skeleton for a
family. It is not implemented in the bootstrap.

## VariationPoint

A `VariationPoint` is an allowed slot where implementations can differ while
remaining inside the same family.

## Evidence

Evidence links a conclusion to a code unit, source range, provenance record, and
note. Every future family conclusion must carry auditable source evidence.
Family evidence storage must remain linked to a family and same-generation code
unit and must carry explicit covered-claim labels. The current allowlist is
`canonical`, `support`, `variation`, and `exception`. Current builders emit
`canonical` and `support`, and they may emit one narrow Python `variation`
evidence label when an already-ready family has multiple exact-compatible
framework-anchor support targets. Exception evidence and broader medoid,
template, or counterexample evidence links remain deferred. Semantic-fact
evidence must not be treated as family evidence by itself.

## Provenance

Provenance contains the source path, content hash, and repository revision used
for a conclusion. Content hashes use the exact `sha256:<64 hex chars>` form;
empty, placeholder, or non-SHA-256 values are not auditable evidence. Stale
provenance must not be treated as fresh evidence.

## Counterexample

A counterexample is a source-backed implementation that resembles a family but
violates a meaningful rule. Counterexample storage is deferred.

## CompatibilityResult

Compatibility expresses whether a target can be compared to a family:
compatible, incompatible with reason, or unknown with reason.

## Measurement

Measurement kinds are `MEASURED`, `DERIVED`, `ESTIMATED`, and
`CAUSAL_EXPERIMENT`. `EstimatedPotentialTokenSavings` is a core `ESTIMATED`
measurement for the metric named `estimated_potential_token_savings`. It
records aggregate-compatible estimated baseline tokens, estimated returned
tokens, saturating potential savings, and a caveat that the value is not
measured token savings. Actual `token_savings` remains a separate metric that
requires comparable baseline/treatment session token counts and a measurement
source.

## AbstentionReason

Abstention prevents weak evidence from becoming a false claim. Reasons include
low confidence, competing families, dynamic runtime behavior, and unsupported
targets.

## UNKNOWN governance

`UNKNOWN` is a typed analysis result. It can be blocking, non-blocking,
recoverable, or irreducible depending on which claim is affected and what
evidence could resolve it. Reason codes include dynamic imports, monkey
patching, pytest fixture injection, runtime dependency injection, unresolved
imports, missing project configuration, missing dependencies, framework magic,
macro or preprocessor ambiguity, build variant ambiguity, conflicting facts,
stale evidence, and insufficient support.

The four `UnknownClass` values remain stable public protocol/storage tokens.
Internal policy does not treat them as one semantic axis: `ClaimImpact`
(`Blocking`/`NonBlocking`) alone controls claim suppression, and
`ResolutionClass` (`Recoverable`/`Irreducible`) alone controls registered
recovery readiness. The legacy public class and counters are projections and
must remain serialization-compatible; they are not an internal cross-product.
The public Rust `ClaimUnknown` record also keeps its existing
`pub class: UnknownClass` field. Family decision code converts that field to
`ClaimImpact` only when it is `Blocking` or `NonBlocking`; resolution-only
`Recoverable` and `Irreducible` values are not valid family-impact inputs.
Runtime/execution irreducibility requires exact typed context, while ordinary
macro expansion, compile-command preprocessing, and cfg selection may remain
recoverable through registered static mechanisms.

The canonical taxonomy and propagation rules live in
`docs/specifications/unknowns.md`. New domain behavior that emits or consumes
unknowns must update that file when it introduces a public reason code, class,
recovery code, recovery action, or mechanism bucket.

## Freshness

Freshness connects evidence to content hashes and repository revisions. Unknown
or stale freshness must be represented explicitly.
The current implementation has a file-hash freshness policy for active semantic
facts. It compares stored fact evidence hashes with current source reads before
allowing a fact to become eligible input for future claim builders. Missing or
changed source becomes a blocking `UNKNOWN` with reason `StaleEvidence` and a
`run repogrammar resync` recovery suggestion. Repository-revision and worktree-wide
freshness are still deferred.

## Classification vocabulary

Minimum support only qualifies a cluster for emission; it does not by itself
prove dominance. Every emitted family carries an evidence-backed
`FamilyPrevalence` record and is classified with one of four tokens:

- `DOMINANT_PATTERN`: high coverage of eligible peers with a reliable
  denominator and no competing ready family that rivals it.
- `SUPPORTED_PATTERN`: meets minimum support but does not dominate its eligible
  peers.
- `MINORITY_PATTERN`: covers less than one third of eligible peers, or is
  smaller than a competing ready family of the same key.
- `UNKNOWN_PREVALENCE`: the denominator is unreliable because blocking unknowns
  dominate the peer group.

Insufficient support, competing families below minimum support, dynamic
behavior, and unsupported targets remain typed `UNKNOWN`s that are never emitted
as families. Variation slots and source-backed exceptions are recorded on the
family record itself, not as separate top-level classification tokens.

### `FamilyPrevalence` record

Each emitted family stores metadata-only prevalence counters (never source
text):

- `eligible_peer_count`: the denominator — units of the same `FamilyKey` whose
  supported evidence survived the blocking filter (this cluster, a competing
  cluster, or a sub-support cluster).
- `supported_member_count`: this cluster's support.
- `coverage_ratio`: `supported_member_count / eligible_peer_count`, `None` only
  when the denominator is zero (impossible for an emitted claim; kept for schema
  honesty).
- `competing_ready_family_count`: other ready clusters of the same key.
- `largest_competing_support`: the largest support among those competitors, `0`
  if none.
- `blocked_peer_count`: peers whose support was emptied by a blocking `UNKNOWN`,
  excluded from the denominator but recorded for reliability.
- `unsupported_peer_count`: peers with no role-compatible support facts,
  excluded from the denominator but recorded for reliability.
- `classification_reason`: one deterministic sentence from a fixed template set.

Denominator rule: only peers that could in principle claim membership count
toward `eligible_peer_count`. Blocked and unsupported peers are excluded but
recorded separately; difficult eligible peers are never dropped to inflate a
family's coverage.

### Classification rule

Classification is decided on exact integers (cross-multiplied thresholds) so the
edges are deterministic and float-free. Let `support = supported_member_count`,
`eligible = eligible_peer_count`, and `competitor = largest_competing_support`:

1. `UNKNOWN_PREVALENCE` when `blocked_peer_count > eligible` (unreliable
   denominator).
2. `MINORITY_PATTERN` when `3 * support < eligible` (coverage below one third)
   or `support < competitor`.
3. `DOMINANT_PATTERN` when `5 * support >= 3 * eligible` (coverage at least
   0.6), `support >= 2 * competitor`, and `support >= 2`.
4. `SUPPORTED_PATTERN` otherwise.

Reason templates are fixed sentences such as `coverage 30/30 with no competing
ready family`, `coverage 3/6 without dominant margin`, `support 3 below
competing ready support 6`, `coverage 2/9 below one-third of eligible peers`, or
`blocked peers 4 exceed eligible peers 3`.

## Family identity

A `FamilyKey` is `(language, code_unit_kind, framework_role, normalized_shape)`.
A single key can produce several ready clusters after complete-link clustering,
and each ready cluster is emitted as one family that needs a stable,
human-auditable id.

### Base id

The base id is `family:{language}:{code_unit_kind}:{framework_role}`, where each
segment is lowercased and every non-alphanumeric character is folded to `_`
(`stable_token`).

### Suffix rule

- A key with at most one ready cluster keeps the bare base id. This is the
  common, stable case and must never change for unrelated repository edits.
- A key with two or more ready clusters gives every ready cluster a suffix, so
  no cluster holds the bare base id. This removes base-id rebinding: adding a
  file whose path sorts earlier can no longer silently re-point the base id at a
  different cluster.

Suffixed ids are `family:{language}:{code_unit_kind}:{framework_role}:v{hex}`.
The `v` prefix plus twelve lowercase hex characters cannot collide with the
legacy `cluster_...` suffix token space.

### Suffix hash derivation

`v{hex}` is the literal `v` followed by the first twelve lowercase hex
characters (six bytes) of the SHA-256 digest of a canonical, newline-terminated
serialization:

```
repogrammar.family-suffix.v1
key={language}:{code_unit_kind}:{framework_role}
profile={characteristic feature}   (one line per feature, sorted)
support-family-core={value}        (only when a profile tie is broken; sorted)
ordinal={n}                        (only for a positional tie)
```

The characteristic profile is exactly the feature values that the role's
compatibility rule (`evidence_pair_is_compatible` and its per-language
refinements) requires to be equal across every cluster member — for example
`decorator_shape:` for FastAPI routes, `http_method:` plus `route_path_shape:`
for Flask and axum routes, or `http_method:` plus `route_template_shape:` for
ASP.NET Core routes. Those values are identical across members by construction,
so the suffix is stable under member addition or removal as long as the
cluster's characteristic profile is unchanged.

### Tie handling

Two sibling ready clusters of the same key can share an identical characteristic
profile when the role is constrained only by the universal preconditions (a
shared support family and an equal role). Ties are broken deterministically:

1. Extend the hashed input with the cluster's support-family core — the
   `support_family` values shared by every member. Two distinct clusters can
   only share a non-empty core if they would already have merged, so this
   distinguishes every pair whose cores differ.
2. If two clusters still tie (identical profile and an identical, necessarily
   empty, core), append a positional ordinal by emission order. The ordinal is
   also recorded on the family as classification-independent metadata
   (`slot:family_positional_discriminator`) so its use is observable rather than
   silent. Positional fallback fires only for genuinely indistinguishable
   clusters.

### Collision disambiguation

`stable_token` is lossy, so two distinct keys (for example the roles
`framework:a.b` and `framework:a_b`) can fold onto the same bare base id. Only
single-ready-cluster keys mint a bare base id, so after emission any such
collision is resolved by giving every key in the colliding group a deterministic
`v{hex}` suffix derived from the full raw `FamilyKey` (domain tag
`repogrammar.family-key.v1`); non-colliding keys keep the bare base id. The
build asserts that all emitted family ids are unique.

### Cross-generation identity

Family ids are deterministic for a fixed input set and stable under unrelated
file changes, but they are follow-up handles, not permanent identities: a
cluster re-clustered under a different characteristic profile appears as one
removed id and one added id, not as an in-place rename. Sync and resync JSON
report this as `families_added` and `families_removed` (see
`specifications/cli.md`); consumers must resync and re-resolve family handles
rather than assume an id refers to the same membership across generations.
